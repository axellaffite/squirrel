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
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
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

/** Assets uploading at the same time; each one's chunks go up one after another. */
const val UPLOAD_CONCURRENCY = 3

/** Files read and hashed at the same time ("Preparing"). */
const val HASH_CONCURRENCY = 2

/** The longest a server's 429/503 holds back every upload of the run (that asset waits its own backoff). */
private const val MAX_SHARED_BACKOFF_MS = 60_000L

/**
 * One backup pass: scan → hash ([hashConcurrency] files at a time) → check (≤ 500 per call) →
 * upload what is missing ([uploadConcurrency] assets at a time). Runs never overlap; a run stops
 * early, keeping everything done so far, when [conditions] stop holding or its deadline passes:
 * no new item starts, and the ones in flight finish or stop between chunks.
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
    private val uploadConcurrency: Int = UPLOAD_CONCURRENCY,
    private val hashConcurrency: Int = HASH_CONCURRENCY,
    /** Waits that many milliseconds of [now]'s clock (tests advance a fake one). */
    private val sleep: suspend (Long) -> Unit = { delay(it) },
    /** Between probes of an unreachable server (Android wakes it on network changes). */
    private val network: NetworkWaiter = NetworkWaiter.Delay,
    private val offlineGraceMillis: Long = OFFLINE_GRACE_MS,
) {
    private val mutex = Mutex()
    private val _state = MutableStateFlow<SyncState>(SyncState.Idle)
    val state: StateFlow<SyncState> = _state.asStateFlow()

    /** Thrown inside a run when it must stop between steps. */
    private class Stop(val stopCause: StopCause, val state: SyncState) : Exception()

    /**
     * [persistent]: a foreground run with no deadline. Losing the network (or the server: a VPN
     * paused with the screen off) does not end it: it waits, probing, then resumes where it was.
     */
    private inner class Run(val deadline: Long?, val backup: BackupSettings, val persistent: Boolean) {
        private val counts = MutableStateFlow(RunSummary())
        val summary: RunSummary get() = counts.value

        /** Items finish on several coroutines at once: counts change atomically. */
        fun count(change: (RunSummary) -> RunSummary) = counts.update(change)

        /** Until when the server asked to wait (429/503): no upload starts before it. */
        val busyUntil = MutableStateFlow(0L)

        /** One worker probes while the others wait for its answer. */
        private val offline = Mutex()

        /** Throws [Stop] when the run must end here (after waiting out a network loss, when persistent). */
        suspend fun checkpoint() {
            if (!ready()) throw stop()
        }

        /** Whether the run may go on; a persistent run first waits for the network to come back. */
        suspend fun ready(): Boolean {
            while (true) {
                val reason = conditions.check(backup) ?: return deadline == null || now() < deadline
                if (reason != PauseReason.NoNetwork || !persistent) return false
                awaitServer()
            }
        }

        /**
         * Waits until the phone has a network and the server answers, probing it on a growing
         * delay; after [offlineGraceMillis] the run ends as [ErrorReason.Unreachable]. No attempt
         * is spent on any asset meanwhile.
         */
        suspend fun awaitServer() = offline.withLock {
            val since = now()
            var attempt = 0
            while (true) {
                if (conditions.check(backup) != PauseReason.NoNetwork && reachable()) return@withLock
                if (now() - since >= offlineGraceMillis) throw Stop(StopCause.Error, SyncState.Error(ErrorReason.Unreachable))
                _state.value = SyncState.WaitingForNetwork
                network.await(probeDelay(attempt++))
            }
        }

        /** [call], again once the server is back when a persistent run cannot reach it. */
        suspend fun <T> reaching(call: suspend () -> T): T {
            while (true) {
                try {
                    return call()
                } catch (e: IOException) {
                    if (!persistent) throw e
                    awaitServer()
                }
            }
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
    suspend fun runOnce(deadline: Long? = null, reconcile: Boolean = false, waitForNetwork: Boolean = false): RunSummary {
        if (!mutex.tryLock()) return RunSummary(stoppedBy = StopCause.AlreadyRunning)
        try {
            return withContext(io) { runLocked(deadline, reconcile, waitForNetwork && deadline == null) }
        } finally {
            mutex.unlock()
        }
    }

    private suspend fun runLocked(deadline: Long?, reconcile: Boolean, persistent: Boolean): RunSummary {
        var run: Run? = null
        fun stopped(state: SyncState, cause: StopCause): RunSummary {
            _state.value = state
            return (run?.summary ?: RunSummary()).copy(stoppedBy = cause)
        }
        try {
            run = Run(deadline, settings.backup(), persistent)
            if (source.access() == MediaAccess.None) throw Stop(StopCause.Paused, SyncState.Paused(PauseReason.NoPhotoAccess))
            run.checkpoint()
            _state.value = SyncState.Scanning
            scanner.scan()
            if (reconcile) scanner.reconcile()
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
        val done = MutableStateFlow(0)
        while (true) {
            val batch = queue.nextToHash(HASH_BATCH)
            if (batch.isEmpty()) return
            // Each batch is hashed in full (or stopped) before the next is read: never twice.
            inParallel(batch, hashConcurrency, run) { asset ->
                _state.value = SyncState.Hashing(done.value, total)
                hashOne(asset, run)
                done.update { it + 1 }
            }
        }
    }

    private suspend fun hashOne(asset: QueuedAsset, run: Run) {
        val reader = source.open(asset.deviceAssetId)
        if (reader == null) {
            queue.markSkipped(asset.deviceAssetId)
            return
        }
        reader.use {
            val size = it.size
            val modifiedAt = it.modifiedAt
            queue.setHash(asset.deviceAssetId, hashAsset(it), size, modifiedAt, it.locationStripped)
        }
        run.count { it.copy(hashed = it.hashed + 1) }
    }

    /**
     * Runs [work] on [items], at most [limit] at a time, in order of start. Each item starts only
     * past the run's checkpoint; when it says stop, nothing new starts, the items in flight finish
     * (they watch the conditions themselves), then the stop is thrown. An exception from one
     * item's [work] cancels the others and ends the run, as before.
     */
    private suspend fun <T> inParallel(items: List<T>, limit: Int, run: Run, work: suspend (T) -> Unit) {
        val gate = Semaphore(limit.coerceAtLeast(1))
        var stop: Stop? = null
        coroutineScope {
            for (item in items) {
                gate.acquire()
                try {
                    run.checkpoint()
                } catch (e: Stop) {
                    gate.release()
                    stop = e
                    break
                }
                launch {
                    try {
                        work(item)
                    } finally {
                        gate.release()
                    }
                }
            }
        }
        stop?.let { throw it }
    }

    private suspend fun check(run: Run) {
        while (true) {
            val batch = queue.hashedBatch(CHECK_BATCH)
            if (batch.isEmpty()) return
            run.checkpoint()
            _state.value = SyncState.Checking
            val request = SyncCheckRequest(batch.map { SyncCheckItem(it.deviceAssetId, checkNotNull(it.sha256)) })
            val missing = run.reaching { api.sync.syncCheck(request).bodyOrThrow().missing }
            queue.applyCheck(batch.map { it.deviceAssetId }, missing)
            val absent = missing.toSet()
            batch.filter { it.deviceAssetId !in absent }.forEach { source.backedUp(it.deviceAssetId) }
            run.count { it.copy(checked = it.checked + batch.size) }
        }
    }

    private suspend fun upload(run: Run) {
        // A snapshot: an asset whose backoff expires during this run waits for the next one,
        // and nothing ends the run early except a stop. Each id goes to exactly one worker.
        val candidates = queue.uploadCandidates(now())
        val uploader = Uploader(
            api, transport, source, queue, now, random,
            parallelChunks = 1,
            reportOffline = run.persistent,
            onServerBusy = { until -> run.busyUntil.update { maxOf(it, minOf(until, now() + MAX_SHARED_BACKOFF_MS)) } },
        )
        val done = MutableStateFlow(0)
        val interrupted = MutableStateFlow(false)
        val total = candidates.size
        inParallel(candidates, uploadConcurrency, run) { id ->
            // A persistent run tries the same asset again once the network is back.
            while (true) {
                waitWhileBusy(run)
                val asset = queue.get(id)?.takeIf { it.state == QueueState.Missing || it.state == QueueState.Uploading }
                    ?: return@inParallel
                if (!run.ready()) {
                    interrupted.value = true
                    return@inParallel
                }
                _state.value = SyncState.Uploading(asset.name, done.value, total)
                val progress: (Float) -> Unit = { _state.value = SyncState.Uploading(asset.name, done.value, total, it) }
                when (uploader.upload(asset, shouldContinue = run::mayContinue, onProgress = progress)) {
                    UploadResult.Uploaded -> run.count { it.copy(uploaded = it.uploaded + 1) }
                    UploadResult.Deduplicated -> run.count { it.copy(deduplicated = it.deduplicated + 1) }
                    UploadResult.Failed -> run.count { it.copy(failed = it.failed + 1) }
                    UploadResult.Interrupted -> if (run.persistent) continue else interrupted.value = true
                    UploadResult.Offline -> {
                        run.awaitServer()
                        continue
                    }
                    UploadResult.Skipped, UploadResult.Changed, is UploadResult.RetryLater -> Unit
                }
                done.update { it + 1 }
                return@inParallel
            }
        }
        // An item stopped between chunks; the others in flight have ended too: the run stops.
        if (interrupted.value) throw run.stop()
    }

    /** The server said it is busy: no new upload starts before the time it gave (capped). */
    /** Whether the server answers at all (any HTTP answer, even an error, counts). */
    private suspend fun reachable(): Boolean = try {
        api.health()
        true
    } catch (e: CancellationException) {
        if (!currentCoroutineContext().isActive) throw e
        false // a timeout that cancels internally
    } catch (_: IOException) {
        false
    } catch (_: Exception) {
        true
    }

    private suspend fun waitWhileBusy(run: Run) {
        while (true) {
            val wait = run.busyUntil.value - now()
            if (wait <= 0 || !run.mayContinue()) return
            sleep(minOf(wait, MAX_SHARED_BACKOFF_MS))
        }
    }
}

/** Failures of the network or the server, as opposed to bugs. */
internal fun Throwable.isNetworkFailure(): Boolean =
    this is IOException || this is ApiError || this is ContentConvertException || this is SerializationException
