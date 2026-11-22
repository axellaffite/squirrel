package app.squirrel.shared.sync

import app.cash.turbine.test
import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.settings.SettingsStore
import app.squirrel.shared.testing.FakeAssetSource
import app.squirrel.shared.testing.FakeServer
import app.squirrel.shared.testing.runApiTest
import app.squirrel.shared.testing.testDriver
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** A foreground run that loses the network (a VPN paused with the screen off) and gets it back. */
class OfflineRunTest {
    private val db = createDatabase(testDriver())
    private val queue = Queue(db)
    private val settings = SettingsStore(db)
    private val source = FakeAssetSource()
    private val server = FakeServer().apply { chunkCount = 2 }
    private var clock = 1_000_000L
    private val conditions = object : Conditions {
        var pause: PauseReason? = null
        override fun check(settings: BackupSettings): PauseReason? = pause
    }

    /** The server cannot be reached while true: every request and chunk fails to connect. */
    private var offline = false

    /** Waits the run made between probes; [onWait] can bring the network back. */
    private val waits = mutableListOf<Long>()
    private var onWait: (Int) -> Unit = {}

    init {
        server.onRequest = { _, _ -> if (offline) throw IOException("connection refused") else null }
        server.chunkFailure = { _, _ -> if (offline) IOException("connection reset") else null }
    }

    private val sleeps = mutableListOf<Long>()

    private suspend fun engine(
        source: AssetSource = this.source,
        transport: ChunkTransport = server.transport,
        network: NetworkWaiter? = null,
    ) = SyncEngine(
        source = source,
        api = server.api(),
        queue = queue,
        scanner = Scanner(source, db, queue, settings, now = { clock }),
        transport = transport,
        conditions = conditions,
        settings = settings,
        now = { clock },
        random = { 0.5 },
        io = Dispatchers.Default,
        sleep = { server.locked { sleeps += it; clock += it } },
        network = network ?: NetworkWaiter { millis ->
            server.locked {
                waits += millis
                clock += millis
                onWait(waits.size)
            }
        },
    )

    private fun put(n: Int) = repeat(n) { source.put("images:$it", size = 64L + it) }

    @Test
    fun aForegroundRunWaitsForTheServerThenFinishesWithoutSpendingAttempts() = runApiTest {
        put(6)
        var chunks = 0
        server.chunkFailure = { _, _ ->
            // The VPN pauses after a few chunks, and comes back after three probes.
            if (++chunks == 4) offline = true
            if (offline) IOException("connection reset") else null
        }
        onWait = { n -> if (n == 3) offline = false }
        val engine = engine()

        engine.state.test {
            val run = async { engine.runOnce(waitForNetwork = true) }
            var waited = false
            while (true) {
                val state = awaitItem()
                if (state == SyncState.WaitingForNetwork) waited = true
                if (waited && state == SyncState.Idle) break
            }
            val summary = run.await()
            assertEquals(StopCause.Finished, summary.stoppedBy)
            assertEquals(6, summary.uploaded)
            cancelAndIgnoreRemainingEvents()
        }
        assertEquals(mapOf(QueueState.Done to 6L), queue.counts())
        assertEquals(listOf(5_000L, 10_000L, 20_000L), waits) // probing on a growing delay
        assertTrue((0..5).all { queue.get("images:$it")!!.attempts == 0 }) // the outage cost no attempt
    }

    @Test
    fun itGivesUpAfterThirtyMinutesAndKeepsEverything() = runApiTest {
        put(3)
        offline = true
        val engine = engine()

        val summary = engine.runOnce(waitForNetwork = true)

        assertEquals(StopCause.Error, summary.stoppedBy)
        assertEquals(SyncState.Error(ErrorReason.Unreachable), engine.state.value)
        assertTrue(waits.sum() >= OFFLINE_GRACE_MS)
        assertTrue(waits.all { it <= PROBE_MAX_MS })
        assertEquals(0L, queue.counts()[QueueState.Failed] ?: 0L)
        assertTrue((0..2).all { queue.get("images:$it")!!.attempts == 0 })
    }

    @Test
    fun noNetworkAtAllIsWaitedOutToo() = runApiTest {
        put(2)
        conditions.pause = PauseReason.NoNetwork // the phone itself has no network yet
        onWait = { n -> if (n == 2) conditions.pause = null }

        val summary = engine().runOnce(waitForNetwork = true)

        assertEquals(StopCause.Finished, summary.stoppedBy)
        assertEquals(2, summary.uploaded)
        assertEquals(2, waits.size)
    }

    @Test
    fun anotherReasonStillPausesAForegroundRun() = runApiTest {
        put(2)
        conditions.pause = PauseReason.ChargingRequired
        val engine = engine()

        engine.runOnce(waitForNetwork = true)

        assertEquals(SyncState.Paused(PauseReason.ChargingRequired), engine.state.value)
        assertTrue(waits.isEmpty())
    }

    @Test
    fun aRunWithADeadlineStillEndsOnANetworkLoss() = runApiTest {
        put(2)
        offline = true
        val engine = engine()

        // waitForNetwork only applies to runs without a deadline (foreground ones).
        val summary = engine.runOnce(deadline = clock + 60_000, waitForNetwork = true)

        assertEquals(StopCause.Error, summary.stoppedBy)
        assertEquals(SyncState.Error(ErrorReason.Unreachable), engine.state.value)
        assertTrue(waits.isEmpty())
    }

    @Test
    fun aFileThePhoneCannotReadSpendsAttemptsUntilItFails() = runApiTest {
        put(1)
        var checked = false
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/sync/check") checked = true
            null
        }
        // Hashing reads it fine; from the upload on, the SD card is gone.
        val failing = object : AssetSource by source {
            override suspend fun open(deviceAssetId: String): AssetReader? {
                val inner = source.open(deviceAssetId) ?: return null
                return object : AssetReader by inner {
                    override suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int =
                        if (checked) throw IOException("No such device") else inner.read(position, into, offset, length)
                }
            }
        }
        // A transport that reads the chunk from the file, as the real one does.
        val reading = object : ChunkTransport {
            override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
                reader.read(0, ByteArray(16), 0, 16)
                server.transport.put(session, index, reader)
            }
        }
        val engine = engine(failing, reading)

        repeat(MAX_ATTEMPTS) {
            val summary = engine.runOnce(waitForNetwork = true)
            assertEquals(StopCause.Finished, summary.stoppedBy) // never a wait for the network
            clock += 60 * 60_000L // past the backoff
        }

        assertEquals(QueueState.Failed, queue.get("images:0")!!.state)
        assertEquals(MAX_ATTEMPTS, queue.get("images:0")!!.attempts)
        assertTrue(waits.isEmpty())
    }

    @Test
    fun aRequestThatKeepsFailingWhileTheServerAnswersSpendsAnAttemptInsteadOfLooping() = runApiTest {
        put(1)
        var tries = 0
        server.chunkFailure = { _, _ -> tries++; IOException("connection reset") } // healthz answers all along
        val engine = engine()

        val summary = engine.runOnce(waitForNetwork = true)

        assertEquals(StopCause.Finished, summary.stoppedBy)
        val asset = queue.get("images:0")!!
        assertEquals(1, asset.attempts) // the third try counted as an ordinary failure
        assertEquals(QueueState.Uploading, asset.state) // its session is kept for the next run
        assertEquals(3, tries) // two false alarms, then the attempt: no hot loop
        assertEquals(listOf(PROBE_BASE_MS, PROBE_BASE_MS), sleeps) // each false alarm waited a probe delay
        assertTrue(waits.isEmpty())
    }

    @Test
    fun aSyncCheckThatKeepsFailingWhileTheServerAnswersEndsTheRun() = runApiTest {
        put(1)
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/sync/check") throw IOException("connection reset") else null
        }
        val engine = engine()

        val summary = engine.runOnce(waitForNetwork = true)

        assertEquals(StopCause.Error, summary.stoppedBy)
        assertEquals(SyncState.Error(ErrorReason.Unreachable), engine.state.value)
        assertEquals(2, server.requests.count { it.url.encodedPath == "/api/sync/check" })
    }

    @Test
    fun theStateStaysWaitingWhileAnotherAssetKeepsUploading() = runApiTest {
        put(2)
        server.chunkCount = 4
        var healthDown = true
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/healthz" && healthDown) throw IOException("connection refused") else null
        }
        // The first session's route is down (with the health check); the other one's is fine and slow.
        val transport = object : ChunkTransport {
            override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
                if (session.uploadId == "up-0" && healthDown) throw IOException("connection reset")
                delay(50)
                server.transport.put(session, index, reader)
            }
        }
        val states = mutableListOf<SyncState>()
        var restoredAt = -1
        val engine = engine(
            transport = transport,
            network = NetworkWaiter {
                delay(300) // meanwhile the other asset sends its chunks
                synchronized(states) { restoredAt = states.size }
                healthDown = false
            },
        )
        val collector = launch { engine.state.collect { synchronized(states) { states += it } } }

        val summary = engine.runOnce(waitForNetwork = true)
        collector.cancel()

        assertEquals(2, summary.uploaded)
        val seen = synchronized(states) { states.toList() }
        val from = seen.indexOf(SyncState.WaitingForNetwork)
        assertTrue(from >= 0 && restoredAt > from, "states: $seen")
        // No flip back to "Uploading" (the notification) while a worker waits for the server.
        assertTrue(seen.subList(from, restoredAt).none { it is SyncState.Uploading }, "states: $seen")
    }
}
