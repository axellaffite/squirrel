package app.squirrel.shared.sync

import app.cash.turbine.ReceiveTurbine
import app.cash.turbine.test
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.hash.Sha256
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.settings.SettingsStore
import app.squirrel.shared.testing.BASE_URL
import app.squirrel.shared.testing.FakeAssetSource
import app.squirrel.shared.testing.FakeServer
import app.squirrel.shared.testing.error
import app.squirrel.shared.testing.runApiTest
import app.squirrel.shared.testing.testDriver
import io.ktor.client.engine.mock.MockEngine
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlin.test.Test
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Runnable
import kotlinx.coroutines.currentCoroutineContext
import kotlin.coroutines.ContinuationInterceptor
import kotlin.coroutines.CoroutineContext
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class SyncEngineTest {
    private val testDriverHolder = testDriver()
    private val db = createDatabase(testDriverHolder)
    private val queue = Queue(db)
    private val settings = SettingsStore(db)
    private val source = FakeAssetSource()
    private val server = FakeServer()
    private var clock = 1_000_000L
    private val conditions = object : Conditions {
        var pause: PauseReason? = null
        override fun check(settings: BackupSettings): PauseReason? = pause
    }

    /** Stands for Dispatchers.IO: counts that work was dispatched to it. */
    private val io = object : CoroutineDispatcher() {
        override fun dispatch(context: CoroutineContext, block: Runnable) = Dispatchers.Default.dispatch(context, block)
    }

    private suspend fun engine(api: SquirrelApi? = null, transport: ChunkTransport = server.transport) = SyncEngine(
        source = source,
        api = api ?: server.api(),
        queue = queue,
        scanner = Scanner(source, db, queue, settings, now = { clock }),
        transport = transport,
        conditions = conditions,
        settings = settings,
        now = { clock },
        random = { 0.5 },
        io = io,
    )

    private fun hashOf(id: String): String {
        val reader = source.reader(id)
        return Sha256().apply { update(reader.bytes(0, reader.size.toInt())) }.digestHex()
    }

    private suspend fun ReceiveTurbine<SyncState>.awaitState(match: (SyncState) -> Boolean): SyncState {
        while (true) {
            val state = awaitItem()
            if (match(state)) return state
        }
    }

    @Test
    fun withoutPhotoAccessTheRunPausesBeforeAnyScan() = runApiTest {
        source.access = MediaAccess.None
        source.put("images:1")

        val summary = engine().runOnce()

        assertEquals(StopCause.Paused, summary.stoppedBy)
        assertEquals(0, source.scans)
        assertEquals(0, source.opened)
    }

    @Test
    fun aPassChecksInBatchesOf500AndUploadsOnlyWhatIsMissing() = runApiTest {
        repeat(1200) { source.put("images:$it", size = 64L + it) } // distinct sizes: distinct contents
        repeat(1195) { server.known += hashOf("images:$it") }
        val engine = engine()

        engine.state.test {
            assertEquals(SyncState.Idle, awaitItem())
            val summary = engine.runOnce()
            assertEquals(SyncState.Idle, engine.state.value, "summary: $summary, counts: ${queue.counts()}")
            assertEquals(1200, summary.hashed)
            assertEquals(5, summary.uploaded)
            awaitState { it is SyncState.Uploading }
            awaitState { it == SyncState.Idle }
            cancelAndIgnoreRemainingEvents()
        }

        assertEquals(3, server.checkCalls)
        assertEquals(5, server.requests.count { it.url.encodedPath == "/api/uploads" })
        assertEquals(mapOf(QueueState.Done to 1200L), queue.counts())
    }

    @Test
    fun theSourceHearsOfEveryAssetTheServerNowHas() = runApiTest {
        repeat(3) { source.put("images:$it", size = 64L + it) }
        server.known += hashOf("images:0") // already there: found by the check
        engine().runOnce()
        assertEquals(setOf("images:0", "images:1", "images:2"), source.backedUp.toSet())
        assertEquals(3, source.backedUp.size)
    }

    @Test
    fun assetsAlreadyDoneAreNeverCheckedAgain() = runApiTest {
        repeat(3) { source.put("images:$it", size = 64) }
        val engine = engine()
        engine.runOnce()
        val checks = server.checkCalls

        engine.runOnce()

        assertEquals(checks, server.checkCalls)
        assertEquals(6, source.opened) // 3 hashes + 3 uploads in the first run, nothing in the second
    }

    @Test
    fun conditionsTurningFalseStopAfterTheCurrentChunk() = runApiTest {
        source.put("images:1", size = 64)
        source.put("images:2", size = 64)
        val transport = object : ChunkTransport {
            override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
                server.transport.put(session, index, reader)
                server.locked { conditions.pause = PauseReason.WifiRequired } // Wi-Fi lost during the first chunk
            }
        }
        val engine = engine(transport = transport)

        val summary = engine.runOnce()

        assertEquals(SyncState.Paused(PauseReason.WifiRequired), engine.state.value)
        // The chunks already in flight (two at a time) finish; no new one starts.
        assertTrue(server.sent.size in 1..2)
        assertEquals(0, summary.uploaded)
        assertEquals(1L, queue.counts()[QueueState.Uploading])
    }

    @Test
    fun unmetConditionsPauseBeforeAnything() = runApiTest {
        source.put("images:1")
        conditions.pause = PauseReason.ChargingRequired
        val engine = engine()

        engine.runOnce()

        assertEquals(SyncState.Paused(PauseReason.ChargingRequired), engine.state.value)
        assertTrue(server.requests.isEmpty())
        assertEquals(null, queue.get("images:1"))
    }

    @Test
    fun theDeadlineStopsCleanly() = runApiTest {
        repeat(3) { source.put("images:$it", size = 64) }
        val transport = object : ChunkTransport {
            override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
                server.transport.put(session, index, reader)
                server.locked { clock += 1_000 }
            }
        }
        val engine = engine(transport = transport)

        val summary = engine.runOnce(deadline = clock + 2_500)

        assertEquals(StopCause.Deadline, summary.stoppedBy)
        assertEquals(SyncState.Idle, engine.state.value)
        assertTrue(summary.uploaded < 3)
        assertEquals(3L, queue.counts().filterKeys { it != QueueState.Done }.values.sum() + summary.uploaded)
    }

    @Test
    fun aFullServerPausesTheRunAndLeavesTheRestUntouched() = runApiTest {
        repeat(3) { source.put("images:$it", size = 64) }
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/uploads") error(HttpStatusCode.InsufficientStorage, "quota_exceeded") else null
        }
        val engine = engine()

        engine.runOnce()

        assertEquals(SyncState.Paused(PauseReason.StorageFull), engine.state.value)
        assertEquals(3L, queue.counts()[QueueState.Missing])
    }

    @Test
    fun runsDoNotOverlap() = runApiTest {
        source.put("images:1", size = 64)
        val gate = CompletableDeferred<Unit>()
        val transport = object : ChunkTransport {
            override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
                gate.await()
                server.transport.put(session, index, reader)
            }
        }
        val engine = engine(transport = transport)

        val first = async { engine.runOnce() }
        engine.state.test { awaitState { it is SyncState.Uploading }; cancelAndIgnoreRemainingEvents() }
        val second = engine.runOnce()
        gate.complete(Unit)

        assertEquals(StopCause.AlreadyRunning, second.stoppedBy)
        assertEquals(1, first.await().uploaded)
    }

    @Test
    fun aRefusedRefreshStopsAsSignedOut() = runApiTest {
        source.put("images:1", size = 64)
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa_a", "sqr_a")) }
        val api = SquirrelApi(BASE_URL, MockEngine { error(HttpStatusCode.Unauthorized, "unauthorized") }, session)
        val engine = engine(api = api)

        engine.runOnce()

        assertEquals(SyncState.Error(ErrorReason.SignedOut), engine.state.value)
        assertEquals(app.squirrel.shared.auth.SessionState.SignedOut, session.state.value)
    }

    @Test
    fun anAssetThatCannotBeReadIsSkippedWhileHashing() = runApiTest {
        source.put("images:1", size = 64)
        source.put("images:2", size = 64)
        source.unreadable += "images:1" // listed by the scan, gone when opened
        val engine = engine()

        val summary = engine.runOnce()

        assertEquals(QueueState.Skipped, queue.get("images:1")!!.state)
        assertEquals(QueueState.Done, queue.get("images:2")!!.state)
        assertEquals(1, summary.uploaded)
    }

    @Test
    fun aBackoffExpiringMidRunDoesNotEndTheRun() = runApiTest {
        repeat(3) { source.put("images:$it", size = 64L + it, modifiedAt = 10L - it) } // upload order 0, 1, 2
        var refusals = 0
        server.onRequest = { request, _ ->
            // The first upload is refused once; the clock then passes its backoff during the next.
            if (request.url.encodedPath == "/api/uploads" && refusals++ == 0) {
                error(HttpStatusCode.ServiceUnavailable, "unavailable")
            } else {
                null
            }
        }
        val transport = object : ChunkTransport {
            override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
                server.transport.put(session, index, reader)
                server.locked { clock += 10_000 }
            }
        }
        val engine = engine(transport = transport)

        val summary = engine.runOnce()

        assertEquals(StopCause.Finished, summary.stoppedBy)
        assertEquals(2, summary.uploaded)
        assertEquals(QueueState.Missing, queue.get("images:0")!!.state)
        assertEquals(clock.toString(), settings.value(LAST_RUN_KEY))
    }

    @Test
    fun onlyAFinishedRunStampsTheLastRun() = runApiTest {
        source.put("images:1", size = 64)
        conditions.pause = PauseReason.WifiRequired

        engine().runOnce()

        assertEquals(null, settings.value(LAST_RUN_KEY))
    }

    @Test
    fun aFailureBeforeTheRunStartsDoesNotKeepTheLock() = runApiTest {
        val engine = engine()
        val driver = testDriverHolder
        driver.execute(null, "DROP TABLE setting", 0) // settings.backup() now throws

        assertFailsWith<Exception> { engine.runOnce() }
        assertEquals(SyncState.Error(ErrorReason.Unexpected), engine.state.value)
        // A second run is not refused as "already running".
        assertFailsWith<Exception> { engine.runOnce() }
    }

    @Test
    fun anUnreachableServerIsNotAnUnexpectedError() = runApiTest {
        source.put("images:1", size = 64)
        server.onRequest = { _, _ -> throw kotlinx.io.IOException("connection refused") }
        val engine = engine()

        val summary = engine.runOnce()

        assertEquals(StopCause.Error, summary.stoppedBy)
        assertEquals(SyncState.Error(ErrorReason.Unreachable), engine.state.value)
    }

    @Test
    fun runsExecuteOnTheIoDispatcher() = runApiTest {
        source.put("images:1", size = 64)
        val dispatchers = mutableSetOf<Any?>()
        val transport = object : ChunkTransport {
            override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
                val dispatcher = currentCoroutineContext()[ContinuationInterceptor]
                server.locked { dispatchers += dispatcher }
                server.transport.put(session, index, reader)
            }
        }

        engine(transport = transport).runOnce()

        assertEquals(setOf<Any?>(io), dispatchers)
    }

    @Test
    fun hashingRecordsWhatWasActuallyRead() = runApiTest {
        source.put("images:1", size = 64, modifiedAt = 42)
        server.known += hashOf("images:1")

        engine().runOnce()

        val asset = queue.get("images:1")!!
        assertEquals(64L, asset.hashedSize)
        assertEquals(42L, asset.hashedModifiedAt)
    }

    @Test
    fun awaitIdleWaitsForTheRunningPass() = runApiTest {
        source.put("images:1", size = 64)
        val gate = CompletableDeferred<Unit>()
        val transport = object : ChunkTransport {
            override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
                gate.await()
                server.transport.put(session, index, reader)
            }
        }
        val engine = engine(transport = transport)
        val run = async { engine.runOnce() }
        engine.state.test { awaitState { it is SyncState.Uploading }; cancelAndIgnoreRemainingEvents() }

        val waiter = async { engine.awaitIdle(); engine.state.value }
        gate.complete(Unit)

        assertEquals(1, run.await().uploaded)
        assertEquals(SyncState.Idle, waiter.await())
    }

    @Test
    fun anAssetReadWithoutItsLocationIsMarked() = runApiTest {
        source.put("images:1", size = 64, locationStripped = true)
        server.known += hashOf("images:1")

        engine().runOnce()

        assertEquals(true, queue.get("images:1")!!.locationStripped)
    }
}
