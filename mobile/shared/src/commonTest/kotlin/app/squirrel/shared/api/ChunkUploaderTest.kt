package app.squirrel.shared.api

import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import app.squirrel.shared.testing.BASE_URL
import app.squirrel.shared.testing.FakeReader
import app.squirrel.shared.testing.error
import app.squirrel.shared.testing.runApiTest
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.request.HttpRequestData
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.utils.io.ByteChannel
import io.ktor.utils.io.readBuffer
import kotlinx.coroutines.launch
import kotlinx.io.readByteArray
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class ChunkUploaderTest {
    private val chunkSize = 8L * 1024 * 1024
    private val reader = FakeReader(size = 20L * 1024 * 1024)

    private suspend fun sentBytes(request: HttpRequestData): ByteArray {
        val content = request.body as OutgoingContent.WriteChannelContent
        val channel = ByteChannel()
        return kotlinx.coroutines.coroutineScope {
            launch {
                content.writeTo(channel)
                channel.flushAndClose()
            }
            channel.readBuffer().readByteArray()
        }
    }

    private suspend fun harness(): Pair<SquirrelApi, MutableList<Pair<HttpRequestData, ByteArray>>> {
        val seen = mutableListOf<Pair<HttpRequestData, ByteArray>>()
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa_a", "sqr_a")) }
        val api = SquirrelApi(
            BASE_URL,
            MockEngine { request ->
                seen += request to sentBytes(request)
                respond("", HttpStatusCode.NoContent)
            },
            session,
        )
        return api to seen
    }

    @Test
    fun streamsTheThirdSliceWithItsExactLengthUnderTheBearer() = runApiTest {
        val (api, seen) = harness()

        api.chunks.put("u1", index = 2, chunkSize = chunkSize, reader = reader, auth = ChunkAuth.Bearer)

        val (request, bytes) = seen.single()
        assertEquals(HttpMethod.Put, request.method)
        assertEquals("/api/uploads/u1/chunks/2", request.url.encodedPath)
        assertEquals("Bearer sqa_a", request.headers[HttpHeaders.Authorization])
        val content = request.body
        assertEquals(ContentType.Application.OctetStream, content.contentType)
        val last = reader.size - 2 * chunkSize
        assertEquals(last, content.contentLength)
        assertContentEquals(reader.bytes(2 * chunkSize, last.toInt()), bytes)
    }

    @Test
    fun usesTheUploadTokenSchemeWhenGivenOne() = runApiTest {
        val (api, seen) = harness()

        api.chunks.put("u1", index = 0, chunkSize = chunkSize, reader = reader, auth = ChunkAuth.UploadToken("squ_t"))

        val (request, bytes) = seen.single()
        assertEquals("Upload squ_t", request.headers[HttpHeaders.Authorization])
        assertEquals(chunkSize, request.body.contentLength)
        assertContentEquals(reader.bytes(0, chunkSize.toInt()), bytes)
    }

    @Test
    fun aRefusedChunkIsAnApiError() = runApiTest {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa_a", "sqr_a")) }
        val api = SquirrelApi(BASE_URL, MockEngine { error(HttpStatusCode.NotFound, "not_found") }, session)

        val failure = assertFailsWith<ApiError> {
            api.chunks.put("gone", index = 0, chunkSize = chunkSize, reader = reader, auth = ChunkAuth.Bearer)
        }
        assertEquals(404, failure.status)
    }
}
