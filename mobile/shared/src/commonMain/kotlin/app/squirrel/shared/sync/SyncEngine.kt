package app.squirrel.shared.sync

import app.squirrel.api.models.SyncCheckItem
import app.squirrel.api.models.SyncCheckRequest
import app.squirrel.shared.api.SignedOut
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.hash.hashAsset
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.settings.SettingsStore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlin.random.Random

/** The server's limit on one sync check. */
const val CHECK_BATCH = 500L
private const val HASH_BATCH = 50L

/**
 * One backup pass: scan → hash → check (≤ 500 per call) → upload what is missing. Runs never
 * overlap; a run stops early, keeping everything done so far, when [conditions] stop holding or
 * its deadline passes.
 */
class SyncEngine(
    private val source: AssetSource,
    private val api: SquirrelApi,
    private val queue: Queue,
    private val scanner: Scanner,
    private val transport: ChunkTransport,
    private val conditions: Conditions,
    private val settings: SettingsStore,
    private val now: () -> Long,
    private val random: () -> Double = { Random.nextDouble() },
) {
    private val mutex = Mutex()
    private val _state = MutableStateFlow<SyncState>(SyncState.Idle)
    val state: StateFlow<SyncState> = _state.asStateFlow()

    /** Thrown inside a run when it must stop between steps. */
    private class Stop(val stopCause: StopCause, val state: SyncState) : Exception()

    private inner class Run(val deadline: Long?, val backup: BackupSettings) {
        var summary = RunSummary()

        /** Throws [Stop] when the run must end here. */
        fun checkpoint() {
            if (!mayContinue()) throw stop()
        }

        fun mayContinue(): Boolean =
            conditions.check(backup) == null && (deadline == null || now() < deadline)

        fun stop(): Stop {
            val reason = conditions.check(backup)
            return if (reason != null) Stop(StopCause.Paused, SyncState.Paused(reason))
            else Stop(StopCause.Deadline, SyncState.Idle)
        }
    }

    suspend fun runOnce(deadline: Long? = null): RunSummary {
        if (!mutex.tryLock()) return RunSummary(stoppedBy = StopCause.AlreadyRunning)
        val run = Run(deadline, settings.backup())
        try {
            if (source.access() == MediaAccess.None) throw Stop(StopCause.Paused, SyncState.Paused(PauseReason.NoPhotoAccess))
            run.checkpoint()
            _state.value = SyncState.Scanning
            scanner.scan()
            hash(run)
            check(run)
            upload(run)
            settings.setValue(LAST_RUN_KEY, now().toString())
            _state.value = SyncState.Idle
            return run.summary
        } catch (e: Stop) {
            _state.value = e.state
            return run.summary.copy(stoppedBy = e.stopCause)
        } catch (e: CancellationException) {
            _state.value = SyncState.Idle
            throw e
        } catch (e: RunStopped) {
            _state.value = SyncState.Error(
                if (e.reason == StopReason.QuotaExceeded) ErrorReason.QuotaExceeded else ErrorReason.StorageFull,
            )
            return run.summary.copy(stoppedBy = StopCause.Error)
        } catch (_: SignedOut) {
            _state.value = SyncState.Error(ErrorReason.SignedOut)
            return run.summary.copy(stoppedBy = StopCause.Error)
        } catch (_: Exception) {
            _state.value = SyncState.Error(ErrorReason.Unreachable)
            return run.summary.copy(stoppedBy = StopCause.Error)
        } finally {
            mutex.unlock()
        }
    }

    private suspend fun hash(run: Run) {
        val total = (queue.counts()[QueueState.New] ?: 0L).toInt()
        var done = 0
        while (true) {
            val batch = queue.nextToHash(HASH_BATCH)
            if (batch.isEmpty()) return
            for (asset in batch) {
                run.checkpoint()
                _state.value = SyncState.Hashing(done, total)
                val reader = source.open(asset.deviceAssetId)
                if (reader == null) {
                    queue.markSkipped(asset.deviceAssetId)
                } else {
                    val sha = reader.use { hashAsset(it) }
                    queue.setHash(asset.deviceAssetId, sha)
                    run.summary = run.summary.copy(hashed = run.summary.hashed + 1)
                }
                done++
            }
        }
    }

    private suspend fun check(run: Run) {
        while (true) {
            val batch = queue.hashedBatch(CHECK_BATCH)
            if (batch.isEmpty()) return
            run.checkpoint()
            _state.value = SyncState.Checking
            val request = SyncCheckRequest(batch.map { SyncCheckItem(it.deviceAssetId, checkNotNull(it.sha256)) })
            val missing = api.sync.syncCheck(request).bodyOrThrow().missing
            queue.applyCheck(batch.map { it.deviceAssetId }, missing)
            run.summary = run.summary.copy(checked = run.summary.checked + batch.size)
        }
    }

    private suspend fun upload(run: Run) {
        val counts = queue.counts()
        val total = ((counts[QueueState.Missing] ?: 0L) + (counts[QueueState.Uploading] ?: 0L)).toInt()
        val uploader = Uploader(api, transport, source, queue, now, random)
        var done = 0
        val attempted = mutableSetOf<String>()
        while (true) {
            val asset = queue.nextToUpload(now()) ?: return
            if (!attempted.add(asset.deviceAssetId)) return // retried later, not in this run
            run.checkpoint()
            _state.value = SyncState.Uploading(asset.name, done, total)
            when (uploader.upload(asset, shouldContinue = run::mayContinue)) {
                UploadResult.Uploaded -> run.summary = run.summary.copy(uploaded = run.summary.uploaded + 1)
                UploadResult.Deduplicated -> run.summary = run.summary.copy(deduplicated = run.summary.deduplicated + 1)
                UploadResult.Failed -> run.summary = run.summary.copy(failed = run.summary.failed + 1)
                UploadResult.Interrupted -> throw run.stop()
                UploadResult.Skipped, UploadResult.Changed, is UploadResult.RetryLater -> Unit
            }
            done++
        }
    }
}
