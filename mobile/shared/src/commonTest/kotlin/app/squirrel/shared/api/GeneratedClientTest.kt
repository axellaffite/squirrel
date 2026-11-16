package app.squirrel.shared.api

import app.squirrel.api.models.DevicePlatform
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.SessionState
import app.squirrel.shared.testing.BASE_URL
import app.squirrel.shared.testing.NODE_JSON
import app.squirrel.shared.testing.error
import app.squirrel.shared.testing.json
import app.squirrel.shared.testing.loginJson
import app.squirrel.shared.testing.runApiTest
import app.squirrel.shared.testing.tokensJson
import app.squirrel.shared.testing.uploadSessionJson
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.OutgoingContent
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.delay
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.concurrent.atomics.AtomicInt
import kotlin.concurrent.atomics.ExperimentalAtomicApi
import kotlin.concurrent.atomics.incrementAndFetch
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(ExperimentalAtomicApi::class)
class GeneratedClientTest {
    private val device = DeviceDetails(name = "Pixel 8", platform = DevicePlatform.android, model = "Pixel 8", appVersion = "0.1.0")

    private class Harness(handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData) {
        val requests = mutableListOf<HttpRequestData>()
        val store = InMemoryTokenStore()
        val session = Session(store)
        val api = SquirrelApi(
            baseUrl = BASE_URL,
            engine = MockEngine { request ->
                requests += request
                handler(request)
            },
            session = session,
        )
    }

    private fun bodyText(request: HttpRequestData): String =
        (request.body as OutgoingContent.ByteArrayContent).bytes().decodeToString()

    @Test
    fun deviceLoginSendsTheGeneratedRequestAndStoresTheTokens() = runApiTest {
        val h = Harness { json(loginJson(), HttpStatusCode.Created) }

        val result = h.api.signIn("ada", "secret", device)

        val request = h.requests.single()
        assertEquals(HttpMethod.Post, request.method)
        assertEquals("/api/auth/device/login", request.url.encodedPath)
        val body = Json.parseToJsonElement(bodyText(request)).jsonObject
        assertEquals("ada", body["username"]!!.jsonPrimitive.content)
        assertEquals("android", body["device"]!!.jsonObject["platform"]!!.jsonPrimitive.content)
        assertNull(request.headers[HttpHeaders.Authorization])
        assertEquals("Ada", result.account.displayName)
        assertEquals("sqr_first", h.store.load())
        assertIs<SessionState.SignedIn>(h.session.state.value)
    }

    @Test
    fun dataCallsCarryTheAccessToken() = runApiTest {
        val h = Harness { request ->
            when (request.url.encodedPath) {
                "/api/auth/device/login" -> json(loginJson(), HttpStatusCode.Created)
                else -> json("""{"missing":[]}""")
            }
        }
        h.api.signIn("ada", "secret", device)

        h.api.sync.syncCheck(app.squirrel.api.models.SyncCheckRequest(emptyList())).bodyOrThrow()

        assertEquals("Bearer sqa_first", h.requests.last().headers[HttpHeaders.Authorization])
    }

    @Test
    fun a401RefreshesOnceAndRetries() = runApiTest {
        val refreshes = AtomicInt(0)
        val h = Harness { request ->
            when (request.url.encodedPath) {
                "/api/auth/device/login" -> json(loginJson(), HttpStatusCode.Created)
                "/api/auth/device/refresh" -> {
                    refreshes.incrementAndFetch()
                    json("""{"tokens":${tokensJson("sqa_second", "sqr_second")}}""")
                }
                else ->
                    if (request.headers[HttpHeaders.Authorization] == "Bearer sqa_second") json("""{"folder_id":"f"}""")
                    else error(HttpStatusCode.Unauthorized, "unauthorized")
            }
        }
        h.api.signIn("ada", "secret", device)

        val folder = h.api.sync.syncFolder(app.squirrel.api.models.SyncFolderRequest(2024)).bodyOrThrow()

        assertEquals("f", folder.folderId)
        assertEquals(1, refreshes.load())
        val refresh = h.requests.single { it.url.encodedPath == "/api/auth/device/refresh" }
        assertNull(refresh.headers[HttpHeaders.Authorization])
        assertTrue(bodyText(refresh).contains("sqr_first"))
        assertEquals("sqr_second", h.store.load())
    }

    private companion object {
        const val CONCURRENT = 8
    }

    @Test
    fun concurrent401sShareOneRefresh() = runApiTest {
        val refreshes = AtomicInt(0)
        val h = Harness { request ->
            when (request.url.encodedPath) {
                "/api/auth/device/login" -> json(loginJson(), HttpStatusCode.Created)
                "/api/auth/device/refresh" -> {
                    refreshes.incrementAndFetch()
                    delay(100)
                    json("""{"tokens":${tokensJson("sqa_second", "sqr_second")}}""")
                }
                else ->
                    if (request.headers[HttpHeaders.Authorization] == "Bearer sqa_second") json("""{"folder_id":"f"}""")
                    else error(HttpStatusCode.Unauthorized, "unauthorized")
            }
        }
        h.api.signIn("ada", "secret", device)

        val results = List(CONCURRENT) { year ->
            async { h.api.sync.syncFolder(app.squirrel.api.models.SyncFolderRequest(2024 + year)).bodyOrThrow() }
        }.awaitAll()

        assertEquals(List(CONCURRENT) { "f" }, results.map { it.folderId })
        // One refresh: reusing a rotated refresh token would get the device revoked.
        assertEquals(1, refreshes.load())
        assertEquals("sqr_second", h.store.load())
        val refresh = h.requests.single { it.url.encodedPath == "/api/auth/device/refresh" }
        assertTrue(bodyText(refresh).contains("sqr_first"))
    }

    @Test
    fun aRejectedRefreshSignsOut() = runApiTest {
        val h = Harness { request ->
            when (request.url.encodedPath) {
                "/api/auth/device/login" -> json(loginJson(), HttpStatusCode.Created)
                else -> error(HttpStatusCode.Unauthorized, "unauthorized")
            }
        }
        h.api.signIn("ada", "secret", device)

        assertFailsWith<SignedOut> {
            h.api.sync.syncFolder(app.squirrel.api.models.SyncFolderRequest(2024)).bodyOrThrow()
        }

        assertEquals(SessionState.SignedOut, h.session.state.value)
        assertNull(h.store.load())
    }

    @Test
    fun theErrorEnvelopeBecomesAnApiError() = runApiTest {
        val h = Harness { error(HttpStatusCode.Unauthorized, "invalid_credentials", "Wrong username or password.") }

        val failure = assertFailsWith<ApiError> { h.api.signIn("ada", "nope", device) }

        assertEquals(401, failure.status)
        assertEquals("invalid_credentials", failure.code)
        assertEquals("Wrong username or password.", failure.message)
        assertTrue(h.session.state.value !is SessionState.SignedIn)
        assertNull(h.store.load())
    }

    @Test
    fun createUploadMapsBothSuccessBodies() = runApiTest {
        var dedup = true
        val h = Harness { request ->
            when (request.url.encodedPath) {
                "/api/auth/device/login" -> json(loginJson(), HttpStatusCode.Created)
                else ->
                    if (dedup) json("""{"deduplicated":true,"node":$NODE_JSON}""")
                    else json(uploadSessionJson(), HttpStatusCode.Created)
            }
        }
        h.api.signIn("ada", "secret", device)
        val request = NewUpload(
            parentId = "0193a000-0000-7000-8000-000000000011",
            name = "IMG_0001.jpg",
            size = 20L * 1024 * 1024,
            sha256 = "a".repeat(64),
            takenAt = "2024-07-14T18:30:05+02:00",
            deviceAssetId = "images:1",
        )

        val first = h.api.uploads.create(request)
        dedup = false
        val second = h.api.uploads.create(request)

        assertEquals("IMG_0001.jpg", assertIs<UploadOutcome.Deduplicated>(first).node.name)
        assertEquals(3, assertIs<UploadOutcome.Session>(second).session.chunkCount)
        val body = Json.parseToJsonElement(bodyText(h.requests.last())).jsonObject
        assertEquals("2024-07-14T18:30:05+02:00", body["taken_at"]!!.jsonPrimitive.content)
        assertEquals("images:1", body["device_asset_id"]!!.jsonPrimitive.content)
        assertEquals("Bearer sqa_first", h.requests.last().headers[HttpHeaders.Authorization])
    }

    @Test
    fun aBodylessFailureStillBecomesAnApiError() = runApiTest {
        val h = Harness { respond("", HttpStatusCode.BadGateway) }

        val failure = assertFailsWith<ApiError> { h.api.health() }

        assertEquals(502, failure.status)
        assertEquals("http_502", failure.code)
    }
}
