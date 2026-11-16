package app.squirrel.shared.testing

import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import app.squirrel.shared.sync.AssetReader
import app.squirrel.shared.sync.ChunkTransport
import app.squirrel.shared.sync.UploadSession
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * A scriptable Squirrel for uploader and engine tests: sync folders, sync check, upload sessions
 * that remember received chunks. Override [onRequest] to inject failures.
 */
class FakeServer {
    val requests = mutableListOf<HttpRequestData>()
    val bodies = mutableListOf<String>()
    /** Content hashes the server already has (dedup and sync check). */
    val known = mutableSetOf<String>()
    val sessions = mutableMapOf<String, MutableSet<Int>>()
    var chunkCount = 3
    var checkCalls = 0
    private var nextSession = 0
    var onRequest: (suspend MockRequestHandleScope.(HttpRequestData, String) -> HttpResponseData?)? = null

    suspend fun session(): Session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa_a", "sqr_a")) }

    suspend fun api(): SquirrelApi = SquirrelApi(BASE_URL, MockEngine { handle(it) }, session())

    /** A chunk transport that stores chunks in [sessions] (or throws what [chunkFailure] returns). */
    val transport = object : ChunkTransport {
        override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
            chunkFailure?.invoke(session.uploadId, index)?.let { throw it }
            val received = sessions[session.uploadId] ?: throw ApiError(404, "not_found", "gone")
            sent += session.uploadId to index
            received += index
        }
    }
    val sent = mutableListOf<Pair<String, Int>>()
    var chunkFailure: ((uploadId: String, index: Int) -> Throwable?)? = null

    private suspend fun MockRequestHandleScope.handle(request: HttpRequestData): HttpResponseData {
        requests += request
        val body = (request.body as? OutgoingContent.ByteArrayContent)?.bytes()?.decodeToString().orEmpty()
        bodies += body
        onRequest?.invoke(this, request, body)?.let { return it }
        val path = request.url.encodedPath
        return when {
            path == "/api/sync/folder" -> {
                val year = Json.parseToJsonElement(body).jsonObject["year"]!!.jsonPrimitive.content
                json("""{"folder_id":"folder-$year"}""")
            }
            path == "/api/sync/check" -> {
                checkCalls++
                val items = Json.parseToJsonElement(body).jsonObject["items"]!!.jsonArray.map { it.jsonObject }
                val missing = items.filter { it["sha256"]!!.jsonPrimitive.content !in known }
                    .map { "\"${it["device_asset_id"]!!.jsonPrimitive.content}\"" }
                json("""{"missing":$missing}""")
            }
            path == "/api/uploads" -> {
                val sha = Json.parseToJsonElement(body).jsonObject["sha256"]!!.jsonPrimitive.content
                if (sha in known) {
                    json("""{"deduplicated":true,"node":$NODE_JSON}""")
                } else {
                    val id = "up-${nextSession++}"
                    sessions[id] = mutableSetOf()
                    json(uploadSessionJson(id).replace("\"chunk_count\":3", "\"chunk_count\":$chunkCount"), HttpStatusCode.Created)
                }
            }
            path.endsWith("/complete") -> {
                val id = path.removePrefix("/api/uploads/").removeSuffix("/complete")
                val received = sessions[id] ?: return error(HttpStatusCode.NotFound, "not_found")
                if (received.size < chunkCount) return error(HttpStatusCode.Conflict, "upload_incomplete")
                sessions.remove(id)
                json(NODE_JSON, HttpStatusCode.Created)
            }
            path.startsWith("/api/uploads/") -> {
                val id = path.removePrefix("/api/uploads/")
                val received = sessions[id] ?: return error(HttpStatusCode.NotFound, "not_found")
                json(uploadSessionJson(id, received.sorted()).replace("\"chunk_count\":3", "\"chunk_count\":$chunkCount"))
            }
            else -> error(HttpStatusCode.NotFound, "not_found")
        }
    }
}
