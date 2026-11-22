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

    private suspend fun engine() = SyncEngine(
        source = source,
        api = server.api(),
        queue = queue,
        scanner = Scanner(source, db, queue, settings, now = { clock }),
        transport = server.transport,
        conditions = conditions,
        settings = settings,
        now = { clock },
        random = { 0.5 },
        io = Dispatchers.Default,
        sleep = { server.locked { clock += it } },
        network = NetworkWaiter { millis ->
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
}
