package app.squirrel.shared.api

import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import app.squirrel.shared.testing.BASE_URL
import app.squirrel.shared.testing.runApiTest
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.get
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class BearerScopeTest {
    @Test
    fun theTokenGoesToThisSquirrelOnly() = runApiTest {
        val seen = mutableListOf<HttpRequestData>()
        var refreshes = 0
        val session = Session(InMemoryTokenStore("sqr_1")).apply { signedIn(Tokens("sqa_1", "sqr_1")) }
        val engine = MockEngine { request ->
            seen += request
            if (request.url.encodedPath == "/api/auth/device/refresh") refreshes++
            if (request.url.toString().startsWith("$BASE_URL/")) return@MockEngine respond("{}")
            // Another host refuses with a bearer challenge.
            respond("", HttpStatusCode.Unauthorized, headersOf(HttpHeaders.WWWAuthenticate, "Bearer"))
        }
        val api = SquirrelApi(BASE_URL, engine, session)

        api.httpClient.get("https://elsewhere.example/picture.jpg")
        api.httpClient.get("$BASE_URL:8443/api/files")

        // Other hosts (and this host on another port) never see the token, even on the 401 retry.
        assertTrue(seen.isNotEmpty())
        assertTrue(seen.all { it.headers[HttpHeaders.Authorization] == null })
        assertEquals(0, refreshes)

        seen.clear()
        api.httpClient.get("$BASE_URL/api/files")
        assertEquals("Bearer sqa_1", seen.first().headers[HttpHeaders.Authorization])
    }
}
