package app.squirrel.shared.testing

import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.request.HttpResponseData
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.test.TestResult
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext

const val BASE_URL = "https://squirrel.test"

/** Runs [block] on a real dispatcher: Ktor's timeouts must not see the test's virtual time. */
fun runApiTest(block: suspend CoroutineScope.() -> Unit): TestResult =
    runTest { withContext(Dispatchers.Default) { block() } }

fun MockRequestHandleScope.json(body: String, status: HttpStatusCode = HttpStatusCode.OK): HttpResponseData =
    respond(body, status, headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString()))

fun MockRequestHandleScope.error(status: HttpStatusCode, code: String, message: String = code): HttpResponseData =
    json("""{"error":{"code":"$code","message":"$message"}}""", status)

fun tokensJson(access: String, refresh: String): String =
    """{"access_token":"$access","access_expires_at":"2026-11-16T12:15:00Z",""" +
        """"refresh_token":"$refresh","refresh_expires_at":"2027-02-14T12:00:00Z"}"""

const val ACCOUNT_JSON =
    """{"id":"0193a000-0000-7000-8000-000000000001","username":"ada","display_name":"Ada","role":"admin"}"""

const val DEVICE_JSON =
    """{"id":"0193a000-0000-7000-8000-000000000002","name":"Pixel 8","platform":"android",""" +
        """"model":"Pixel 8","app_version":"0.1.0","created_at":"2026-11-16T12:00:00Z",""" +
        """"last_seen_at":"2026-11-16T12:00:00Z","revoked_at":null,"current":true}"""

fun loginJson(access: String = "sqa_first", refresh: String = "sqr_first"): String =
    """{"device":$DEVICE_JSON,"account":$ACCOUNT_JSON,"tokens":${tokensJson(access, refresh)}}"""

const val NODE_JSON =
    """{"id":"0193a000-0000-7000-8000-000000000010","parent_id":"0193a000-0000-7000-8000-000000000011",""" +
        """"name":"IMG_0001.jpg","kind":"file","size":20971520,"mime":"image/jpeg","is_root":false,""" +
        """"created_at":"2026-11-16T12:00:00Z","modified_at":"2026-11-16T12:00:00Z","thumbnail":null}"""

fun uploadSessionJson(id: String = "0193a000-0000-7000-8000-000000000020", received: List<Int> = emptyList()): String =
    """{"upload_id":"$id","chunk_size":8388608,"chunk_count":3,"received_chunks":$received,""" +
        """"expires_at":"2026-11-17T12:00:00Z"}"""
