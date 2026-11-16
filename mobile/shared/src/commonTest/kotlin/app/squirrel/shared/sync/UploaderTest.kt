package app.squirrel.shared.sync

import app.squirrel.shared.api.ApiError
import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.testing.FakeAssetSource
import app.squirrel.shared.testing.FakeServer
import app.squirrel.shared.testing.error
import app.squirrel.shared.testing.runApiTest
import app.squirrel.shared.testing.testDriver
import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue

class UploaderTest {
    private val db = createDatabase(testDriver())
    private val queue = Queue(db)
    private val source = FakeAssetSource()
    private val server = FakeServer()
    private val now = 1_000_000L

    private suspend fun uploader() = Uploader(server.api(), server.transport, source, queue, now = { now }, random = { 0.5 })

    /** Puts a 20 MiB asset in the queue as `missing` (scanned, hashed, checked). */
    private fun missing(id: String = "images:1", size: Long = 20L * 1024 * 1024): QueuedAsset {
        val asset = source.put(id, size = size, takenAt = "2024-07-14T18:30:05+02:00")
        db.localAssetQueries.insertScanned(id, asset.kind, asset.name, asset.mime, size, 1, asset.takenAt, asset.album, 1)
        queue.setHash(id, "f".repeat(64))
        queue.applyCheck(listOf(id), missing = listOf(id))
        return queue.get(id)!!
    }

    @Test
    fun aFreshUploadSendsEveryChunkAndCompletes() = runApiTest {
        val asset = missing()

        val result = uploader().upload(asset)

        assertEquals(UploadResult.Uploaded, result)
        assertEquals(listOf(0, 1, 2), server.sent.map { it.second }.sorted())
        assertEquals(QueueState.Done, queue.get(asset.deviceAssetId)!!.state)
        val create = server.bodies[server.requests.indexOfFirst { it.url.encodedPath == "/api/uploads" }]
        assertTrue("\"parent_id\":\"folder-2024\"" in create, create)
        assertTrue("\"taken_at\":\"2024-07-14T18:30:05+02:00\"" in create, create)
        assertTrue("\"device_asset_id\":\"images:1\"" in create, create)
        assertTrue(source.reader("images:1").closed)
    }

    @Test
    fun theYearFolderIsAskedOncePerRun() = runApiTest {
        val uploader = uploader()
        uploader.upload(missing("images:1"))
        uploader.upload(missing("images:2"))
        assertEquals(1, server.requests.count { it.url.encodedPath == "/api/sync/folder" })
    }

    @Test
    fun aResumeSendsOnlyTheMissingChunks() = runApiTest {
        val asset = missing()
        server.sessions["up-old"] = mutableSetOf(0)
        queue.markUploading(asset.deviceAssetId, "up-old")

        val result = uploader().upload(queue.get(asset.deviceAssetId)!!)

        assertEquals(UploadResult.Uploaded, result)
        assertEquals(listOf("up-old" to 1, "up-old" to 2), server.sent.sortedBy { it.second })
        assertTrue(server.requests.none { it.url.encodedPath == "/api/uploads" })
    }

    @Test
    fun aDeduplicatedUploadSendsNoChunk() = runApiTest {
        server.known += "f".repeat(64)
        val result = uploader().upload(missing())
        assertEquals(UploadResult.Deduplicated, result)
        assertTrue(server.sent.isEmpty())
        assertEquals(QueueState.Done, queue.get("images:1")!!.state)
    }

    @Test
    fun anExpiredSessionStartsAgain() = runApiTest {
        val asset = missing()
        queue.markUploading(asset.deviceAssetId, "up-expired") // the server no longer knows it

        assertEquals(UploadResult.Uploaded, uploader().upload(queue.get(asset.deviceAssetId)!!))
        assertEquals(setOf("up-0"), server.sent.map { it.first }.toSet())
    }

    @Test
    fun aChunk404MidUploadStartsAgain() = runApiTest {
        var failed = false
        server.chunkFailure = { id, index ->
            if (!failed && index == 1) {
                failed = true
                server.sessions.remove(id)
                ApiError(404, "not_found", "gone")
            } else null
        }
        assertEquals(UploadResult.Uploaded, uploader().upload(missing()))
        assertEquals(QueueState.Done, queue.get("images:1")!!.state)
    }

    @Test
    fun anIncompleteUploadIsCompletedAfterResendingWhatIsMissing() = runApiTest {
        var lost = false
        server.onRequest = { request, _ ->
            if (!lost && request.url.encodedPath.endsWith("/complete")) {
                lost = true
                server.sessions.values.first().remove(2) // the server lost a chunk
            }
            null
        }
        assertEquals(UploadResult.Uploaded, uploader().upload(missing()))
        assertEquals(2, server.sent.count { it.second == 2 })
    }

    @Test
    fun aHashMismatchSendsTheFileBackToHashing() = runApiTest {
        server.onRequest = { request, _ ->
            if (request.url.encodedPath.endsWith("/complete")) error(HttpStatusCode.UnprocessableEntity, "hash_mismatch") else null
        }
        assertEquals(UploadResult.Changed, uploader().upload(missing()))
        val asset = queue.get("images:1")!!
        assertEquals(QueueState.New, asset.state)
        assertNull(asset.sha256)
    }

    @Test
    fun aFullServerStopsTheRun() = runApiTest {
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/uploads") error(HttpStatusCode.InsufficientStorage, "quota_exceeded") else null
        }
        val stop = assertFailsWith<RunStopped> { uploader().upload(missing()) }
        assertEquals(StopReason.QuotaExceeded, stop.reason)
        assertEquals(QueueState.Missing, queue.get("images:1")!!.state)
    }

    @Test
    fun aBusyServerIsRetriedLaterWithBackoff() = runApiTest {
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/uploads") error(HttpStatusCode.ServiceUnavailable, "unavailable") else null
        }
        val result = uploader().upload(missing())

        val asset = queue.get("images:1")!!
        assertEquals(UploadResult.RetryLater(now + 5_000), result)
        assertEquals(1, asset.attempts)
        assertEquals(now + 5_000, asset.nextAttemptAt)
        assertEquals(QueueState.Missing, asset.state)
    }

    @Test
    fun networkErrorsAreRetriedAndTheEighthFailureIsFinal() = runApiTest {
        server.chunkFailure = { _, _ -> IOException("connection reset") }
        val uploader = uploader()
        repeat(7) { uploader.upload(queue.get("images:1") ?: missing()) }
        assertEquals(QueueState.Uploading, queue.get("images:1")!!.state)

        val last = uploader.upload(queue.get("images:1")!!)

        assertEquals(UploadResult.Failed, last)
        val asset = queue.get("images:1")!!
        assertEquals(QueueState.Failed, asset.state)
        assertEquals("connection reset", asset.lastError)
    }

    @Test
    fun anAssetGoneFromThePhoneIsSkipped() = runApiTest {
        val asset = missing()
        source.remove(asset.deviceAssetId)
        assertEquals(UploadResult.Skipped, uploader().upload(asset))
        assertEquals(QueueState.Skipped, queue.get(asset.deviceAssetId)!!.state)
    }

    @Test
    fun anInterruptedUploadKeepsItsSession() = runApiTest {
        val uploader = Uploader(server.api(), server.transport, source, queue, now = { now }, random = { 0.5 }, parallelChunks = 1)
        var allowed = 1
        val result = uploader.upload(missing(), shouldContinue = { allowed-- > 0 })
        assertEquals(UploadResult.Interrupted, result)
        val asset = queue.get("images:1")!!
        assertEquals(QueueState.Uploading, asset.state)
        assertEquals(listOf(0), server.sessions.getValue(asset.uploadId!!).toList())
    }

    @Test
    fun backoffGrowsAndIsCapped() {
        assertEquals(5_000, backoffMillis(attempt = 0, random = { 0.5 }))
        assertEquals(40_000, backoffMillis(attempt = 3, random = { 0.5 }))
        assertEquals(15 * 60_000, backoffMillis(attempt = 20, random = { 0.5 }))
        assertEquals(4_000, backoffMillis(attempt = 0, random = { 0.0 }))
        assertEquals(6_000, backoffMillis(attempt = 0, random = { 1.0 }))
    }

    private fun refuse(path: (String) -> Boolean, status: HttpStatusCode, code: String, message: String = code) {
        server.onRequest = { request, _ -> if (path(request.url.encodedPath)) error(status, code, message) else null }
    }

    @Test
    fun permanentRefusalsFailAtOnceWithAReadableMessage() = runApiTest {
        val cases = listOf(
            Triple(HttpStatusCode.BadRequest, "validation_error", "The name is not valid."),
            Triple(HttpStatusCode.Forbidden, "forbidden", "Uploads are disabled."),
            Triple(HttpStatusCode.UnprocessableEntity, "validation_error", "The file is empty."),
            Triple(HttpStatusCode.Conflict, "name_conflict", "A file with this name exists."),
        )
        cases.forEachIndexed { i, (status, code, message) ->
            refuse({ it == "/api/uploads" }, status, code, message)
            val result = uploader().upload(missing("images:$i"))
            assertEquals(UploadResult.Failed, result, status.toString())
            val asset = queue.get("images:$i")!!
            assertEquals(QueueState.Failed, asset.state)
            assertEquals(message, asset.lastError)
        }
    }

    @Test
    fun aTooLargeChunkFailsWithAPlainMessage() = runApiTest {
        server.chunkFailure = { _, _ -> ApiError(413, "http_413", "Payload Too Large", fromServer = false) }
        assertEquals(UploadResult.Failed, uploader().upload(missing()))
        assertEquals("This file is larger than your Squirrel accepts", queue.get("images:1")!!.lastError)
    }

    @Test
    fun retryAfterIsHonoured() = runApiTest {
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/uploads") {
                respond(
                    """{"error":{"code":"rate_limited","message":"slow down"}}""",
                    HttpStatusCode.TooManyRequests,
                    headersOf(HttpHeaders.ContentType to listOf("application/json"), HttpHeaders.RetryAfter to listOf("120")),
                )
            } else {
                null
            }
        }
        assertEquals(UploadResult.RetryLater(now + 120_000), uploader().upload(missing()))
    }

    @Test
    fun aShortRetryAfterDoesNotShortenTheBackoff() = runApiTest {
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/uploads") {
                respond(
                    """{"error":{"code":"unavailable","message":"busy"}}""",
                    HttpStatusCode.ServiceUnavailable,
                    headersOf(HttpHeaders.ContentType to listOf("application/json"), HttpHeaders.RetryAfter to listOf("1")),
                )
            } else {
                null
            }
        }
        assertEquals(UploadResult.RetryLater(now + 5_000), uploader().upload(missing()))
    }
}
