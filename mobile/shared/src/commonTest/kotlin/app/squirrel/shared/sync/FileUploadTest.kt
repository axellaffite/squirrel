package app.squirrel.shared.sync

import app.squirrel.shared.api.ApiError
import app.squirrel.shared.hash.hashAsset
import app.squirrel.shared.testing.FakeServer
import app.squirrel.shared.testing.error
import app.squirrel.shared.testing.runApiTest
import io.ktor.http.HttpStatusCode
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class FileUploadTest {
    private val bytes = "a picked file".encodeToByteArray()

    @Test
    fun aNewFileSendsEveryChunkThenCompletes() = runApiTest {
        val server = FakeServer()
        val progress = mutableListOf<Float>()
        val node = FileUpload(server.api(), server.transport).upload("folder-1", "notes.txt", ByteArrayReader(bytes)) { progress += it }
        assertEquals("IMG_0001.jpg", node.name)
        assertEquals(listOf(0, 1, 2), server.sent.map { it.second })
        assertTrue(server.requests.any { it.url.encodedPath.endsWith("/complete") })
        val create = server.bodies[server.requests.indexOfFirst { it.url.encodedPath == "/api/uploads" }]
        assertTrue("\"parent_id\":\"folder-1\"" in create)
        assertTrue("\"name\":\"notes.txt\"" in create)
        // A file put into a folder is not a phone backup.
        assertFalse("device_asset_id" in create)
        assertEquals(1f, progress.last())
        assertEquals(progress, progress.sorted())
    }

    @Test
    fun contentTheCallerHasIsLinkedWithoutChunks() = runApiTest {
        val server = FakeServer()
        server.known += hashAsset(ByteArrayReader(bytes))
        FileUpload(server.api(), server.transport).upload("folder-1", "notes.txt", ByteArrayReader(bytes))
        assertTrue(server.sent.isEmpty())
        assertFalse(server.requests.any { it.url.encodedPath.endsWith("/complete") })
    }

    @Test
    fun aBusyCompletionIsTriedAgain() = runApiTest {
        val server = FakeServer()
        var busy = 1
        server.onRequest = { request, _ ->
            if (request.url.encodedPath.endsWith("/complete") && busy-- > 0) error(HttpStatusCode.ServiceUnavailable, "busy") else null
        }
        FileUpload(server.api(), server.transport, completeRetryMs = 1).upload("folder-1", "notes.txt", ByteArrayReader(bytes))
        assertEquals(2, server.requests.count { it.url.encodedPath.endsWith("/complete") })
    }

    @Test
    fun aRefusalReachesTheCaller() = runApiTest {
        val server = FakeServer()
        server.onRequest = { request, _ ->
            if (request.url.encodedPath == "/api/uploads") error(HttpStatusCode.Forbidden, "read_only", "Shared with you for reading only") else null
        }
        val refused = assertFailsWith<ApiError> { FileUpload(server.api(), server.transport).upload("folder-1", "a.txt", ByteArrayReader(bytes)) }
        assertEquals(403, refused.status)
        assertEquals("Shared with you for reading only", refused.message)
    }
}
