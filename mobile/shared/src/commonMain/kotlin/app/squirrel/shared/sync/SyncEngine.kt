package app.squirrel.shared.sync

import app.squirrel.api.models.SyncCheckItem
import app.squirrel.api.models.SyncCheckRequest
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SignedOut
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.hash.hashAsset
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.settings.SettingsStore
import io.ktor.serialization.ContentConvertException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.withContext
import kotlinx.io.IOException
import kotlinx.serialization.SerializationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
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
    /** Where runs execute: database, file and network work never runs on the main thread. */
    private val io: CoroutineDispatcher = Dispatchers.IO,
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

    /** Suspends until no run is in progress. */
    suspend fun awaitIdle() {
        mutex.withLock { }
    }

    /**
     * Network and server failures end the run as [ErrorReason.Unreachable]; anything else is a
     * bug and is rethrown after the state says [ErrorReason.Unexpected].
     */
    suspend fun runOnce(deadline: Long? = null): RunSummary {
        if (!mutex.tryLock()) return RunSummary(stoppedBy = StopCause.AlreadyRunning)
        try {
            return withContext(io) { runLocked(deadline) }
        } finally {
            mutex.unlock()
        }
    }

    private suspend fun runLocked(deadline: Long?): RunSummary {
        var run: Run? = null
        fun stopped(state: SyncState, cause: StopCause): RunSummary {
            _state.value = state
            return (run?.summary ?: RunSummary()).copy(stoppedBy = cause)
        }
        try {
            run = Run(deadline, settings.backup())
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
            return stopped(e.state, e.stopCause)
        } catch (e: CancellationException) {
            _state.value = SyncState.Idle
            throw e
        } catch (_: RunStopped) {
            // Quota or disk: nothing more can go up until the user frees space ("Storage full").
            return stopped(SyncState.Paused(PauseReason.StorageFull), StopCause.Paused)
        } catch (_: SignedOut) {
            return stopped(SyncState.Error(ErrorReason.SignedOut), StopCause.Error)
        } catch (e: Exception) {
            if (e.isNetworkFailure()) return stopped(SyncState.Error(ErrorReason.Unreachable), StopCause.Error)
            _state.value = SyncState.Error(ErrorReason.Unexpected)
            throw e
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
                    reader.use {
                        val size = it.size
                        val modifiedAt = it.modifiedAt
                        queue.setHash(asset.deviceAssetId, hashAsset(it), size, modifiedAt, it.locationStripped)
                    }
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
        // A snapshot: an asset whose backoff expires during this run waits for the next one,
        // and nothing ends the run early except a stop.
        val candidates = queue.uploadCandidates(now())
        val uploader = Uploader(api, transport, source, queue, now, random)
        candidates.forEachIndexed { done, id ->
            val asset = queue.get(id)?.takeIf { it.state == QueueState.Missing || it.state == QueueState.Uploading }
                ?: return@forEachIndexed
            run.checkpoint()
            _state.value = SyncState.Uploading(asset.name, done, candidates.size)
            val progress: (Float) -> Unit = { _state.value = SyncState.Uploading(asset.name, done, candidates.size, it) }
            when (uploader.upload(asset, shouldContinue = run::mayContinue, onProgress = progress)) {
                UploadResult.Uploaded -> run.summary = run.summary.copy(uploaded = run.summary.uploaded + 1)
                UploadResult.Deduplicated -> run.summary = run.summary.copy(deduplicated = run.summary.deduplicated + 1)
                UploadResult.Failed -> run.summary = run.summary.copy(failed = run.summary.failed + 1)
                UploadResult.Interrupted -> throw run.stop()
                UploadResult.Skipped, UploadResult.Changed, is UploadResult.RetryLater -> Unit
            }
        }
    }
}

/** Failures of the network or the server, as opposed to bugs. */
internal fun Throwable.isNetworkFailure(): Boolean =
    this is IOException || this is ApiError || this is ContentConvertException || this is SerializationException
