package app.squirrel.ui.testing

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.ContentType
import io.ktor.http.Headers
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import io.ktor.http.headersOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout

val JSON: Headers = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())

/** A signed-in [SquirrelApi] over a mock server. */
suspend fun testApi(handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData): SquirrelApi {
    val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
    return SquirrelApi("https://home.example", MockEngine(handler), session)
}

fun MockRequestHandleScope.json(body: String, status: HttpStatusCode = HttpStatusCode.OK): HttpResponseData =
    respond(body, status, JSON)

fun MockRequestHandleScope.apiError(status: HttpStatusCode, code: String, message: String = "Refused", headers: Headers = JSON): HttpResponseData =
    respond("""{"error":{"code":"$code","message":"$message"}}""", status, headers)

/** The text of a request body (JSON sent by the generated client). */
fun HttpRequestData.text(): String = when (val content = body) {
    is OutgoingContent.ByteArrayContent -> content.bytes().decodeToString()
    else -> ""
}

/** Runs on a real dispatcher, as the existing view model tests do. */
fun runReal(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

suspend fun <T> StateFlow<T>.await(predicate: (T) -> Boolean): T = withTimeout(5_000) { first(predicate) }

fun summary(id: String, takenAt: String = "2024-07-14T18:30:05") =
    """{"id":"$id","kind":"photo","name":"$id.jpg","taken_at":"$takenAt","thumbnail":{"state":"ready","version":1},"width":4,"height":3}"""

fun node(id: String, kind: String = "file", parent: String? = "root", size: Long = 1000, mime: String? = "application/pdf") =
    """{"id":"$id","parent_id":${parent?.let { "\"$it\"" } ?: "null"},"name":"$id${if (kind == "file") ".pdf" else ""}","kind":"$kind","size":$size,""" +
        """"mime":${if (kind == "file" && mime != null) "\"$mime\"" else "null"},"is_root":false,""" +
        """"created_at":"2026-11-16T12:00:00Z","modified_at":"2026-11-16T12:00:00Z","thumbnail":null}"""
