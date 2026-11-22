package app.squirrel.shared.sync

import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.hash.Sha256
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.settings.SettingsStore
import app.squirrel.shared.testing.FakeAssetSource
import app.squirrel.shared.testing.FakeServer
import app.squirrel.shared.testing.error
import app.squirrel.shared.testing.runApiTest
import app.squirrel.shared.testing.testDriver
import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeout
import kotlin.concurrent.atomics.AtomicInt
import kotlin.concurrent.atomics.ExperimentalAtomicApi
import kotlin.concurrent.atomics.decrementAndFetch
import kotlin.concurrent.atomics.incrementAndFetch
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** Several assets (and hashes) at once: limits, isolation, pauses, cancellation, shared backoff. */
@OptIn(ExperimentalAtomicApi::class)
class ParallelRunTest {
    private val db = createDatabase(testDriver())
    private val queue = Queue(db)
    private val settings = SettingsStore(db)
    private val library = FakeAssetSource()
    private val server = FakeServer().apply { chunkCount = 2 }
    private var clock = 1_000_000L
    private val sleeps = mutableListOf<Long>()
    private val conditions = object : Conditions {
        var pause: PauseReason? = null
        override fun check(settings: BackupSettings): PauseReason? = pause
    }

    /** Readers open at once, and the most seen together. */
    private val reading = AtomicInt(0)
    private val mostReading = AtomicInt(0)

    /** Counts readers between open and close; each first read takes a little while. */
    private val source = object : AssetSource by library {
        override suspend fun open(deviceAssetId: String): AssetReader? {
            val inner = library.open(deviceAssetId) ?: return null
            val now = reading.incrementAndFetch()
            while (true) {
                val most = mostReading.load()
                if (now <= most || mostReading.compareAndSet(most, now)) break
            }
            return object : AssetReader by inner {
                private var first = true
                override suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int {
                    if (first) delay(20).also { first = false }
                    return inner.read(position, into, offset, length)
                }

                override fun close() {
                    inner.close()
                    reading.decrementAndFetch()
                }
            }
        }
    }

    /** Chunks in flight at once, and the most seen together (several assets). */
    private var putting = 0
    private var mostPutting = 0

    private fun slowTransport(before: suspend (UploadSession, Int) -> Unit = { _, _ -> }) = object : ChunkTransport {
        override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
            server.locked { putting++; mostPutting = maxOf(mostPutting, putting) }
            try {
                before(session, index)
                delay(30)
                server.transport.put(session, index, reader)
            } finally {
                server.locked { putting-- }
            }
        }
    }

    private suspend fun engine(transport: ChunkTransport = slowTransport(), uploads: Int = UPLOAD_CONCURRENCY) = SyncEngine(
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
        uploadConcurrency = uploads,
        sleep = { server.locked { sleeps += it; clock += it } },
    )

    private fun put(n: Int) = repeat(n) { library.put("images:$it", size = 64L + it) }

    private fun hashOf(id: String): String {
        val reader = library.reader(id)
        return Sha256().apply { update(reader.bytes(0, reader.size.toInt())) }.digestHex()
    }

    private val completes get() = server.requests.count { it.url.encodedPath.endsWith("/complete") }

    @Test
    fun atMostThreeAssetsUploadAtOnceAndEveryOneArrives() = runApiTest {
        put(9)
        val summary = engine().runOnce()
        assertEquals(9, summary.uploaded)
        assertEquals(3, mostPutting) // three assets, each sending its chunks one after another
        assertEquals(mapOf(QueueState.Done to 9L), queue.counts())
        assertEquals(9, completes)
    }

    @Test
    fun atMostTwoFilesArePreparedAtOnce() = runApiTest {
        put(8)
        repeat(8) { server.known += hashOf("images:$it") } // nothing to upload: only hashing reads
        val summary = engine().runOnce()
        assertEquals(8, summary.hashed)
        assertEquals(2, mostReading.load())
    }

    @Test
    fun aRefusedAssetDoesNotHoldBackTheOthers() = runApiTest {
        put(5)
        val refused = hashOf("images:2")
        server.onRequest = { request, body ->
            if (request.url.encodedPath == "/api/uploads" && refused in body) error(HttpStatusCode.UnprocessableEntity, "invalid", "Not a photo")
            else null
        }
        val summary = engine().runOnce()
        assertEquals(StopCause.Finished, summary.stoppedBy)
        assertEquals(4, summary.uploaded)
        assertEquals(1, summary.failed)
        assertEquals(QueueState.Failed, queue.get("images:2")!!.state)
        assertEquals(4L, queue.counts()[QueueState.Done])
    }

    @Test
    fun aPauseMidBatchStartsNothingNewAndLosesNothing() = runApiTest {
        put(9)
        var wifiDrops = true
        val transport = slowTransport { _, _ ->
            // Wi-Fi goes after the first few chunks.
            server.locked { if (wifiDrops && server.sent.size >= 2) conditions.pause = PauseReason.WifiRequired }
        }
        val engine = engine(transport)
        val summary = engine.runOnce()

        assertEquals(SyncState.Paused(PauseReason.WifiRequired), engine.state.value)
        assertEquals(StopCause.Paused, summary.stoppedBy)
        val counts = queue.counts()
        assertEquals(summary.uploaded.toLong(), counts[QueueState.Done] ?: 0L) // counted once each
        assertEquals(9L, counts.values.sum())
        assertTrue((counts[QueueState.Uploading] ?: 0L) <= UPLOAD_CONCURRENCY) // only those in flight
        assertTrue(summary.uploaded < 9)

        wifiDrops = false
        conditions.pause = null
        engine.runOnce() // the next run resumes the sessions and uploads the rest, once
        assertEquals(mapOf(QueueState.Done to 9L), queue.counts())
        assertEquals(9, completes)
    }

    @Test
    fun aCancelledRunLosesNothingAndUploadsNothingTwice() = runApiTest {
        put(9)
        val started = CompletableDeferred<Unit>()
        val engine = engine(slowTransport { _, _ -> started.complete(Unit) })
        val run = async { engine.runOnce() }
        withTimeout(5_000) { started.await() }
        delay(50)
        run.cancelAndJoin() // the system stopped the worker

        assertEquals(SyncState.Idle, engine.state.value)
        assertEquals(9L, queue.counts().values.sum())
        assertEquals(0L, queue.counts()[QueueState.Failed] ?: 0L)

        engine.runOnce()
        assertEquals(mapOf(QueueState.Done to 9L), queue.counts())
        assertEquals(9, completes) // a session cut short is resumed, never completed twice
    }

    @Test
    fun aBusyServerHoldsBackEveryWorker() = runApiTest {
        put(6)
        var busy = true
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/uploads" && busy) {
                busy = false
                respond(
                    """{"error":{"code":"rate_limited","message":"Slow down"}}""",
                    HttpStatusCode.TooManyRequests,
                    headersOf("Content-Type" to listOf("application/json"), "Retry-After" to listOf("30")),
                )
            } else {
                null
            }
        }
        val summary = engine().runOnce()

        assertEquals(5, summary.uploaded)
        // Whoever started after the 429 waited for its Retry-After first.
        assertEquals(30_000L, sleeps.firstOrNull())
    }
}
