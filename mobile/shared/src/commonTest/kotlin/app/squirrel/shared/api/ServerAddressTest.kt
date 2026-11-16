package app.squirrel.shared.api

import app.squirrel.shared.testing.runApiTest
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ServerAddressTest {
    private fun ok(input: String) = normalizeServerUrl(input).getOrThrow()
    private fun refusal(input: String) = normalizeServerUrl(input).exceptionOrNull()?.message

    @Test
    fun privateAndVpnHostsMayUseHttp() {
        listOf(
            "10.1.2.3", "172.16.0.1", "172.31.255.255", "192.168.11.218", "100.64.0.1", "100.127.255.254",
            "127.0.0.1", "localhost", "nas.local", "router.lan", "nas.home.arpa", "nas.tail1234.ts.net",
            "NAS.LOCAL.", "fd12:3456::1", "fe80::1", "::1",
        ).forEach { assertTrue(isPrivateHost(it), it) }
    }

    @Test
    fun publicHostsMayNot() {
        listOf(
            "8.8.8.8", "172.32.0.1", "172.15.0.1", "100.128.0.1", "192.169.0.1", "11.0.0.1",
            "squirrel.example.com", "nas", "local", "evil.local.example.com", "2001:db8::1", "ts.net",
        ).forEach { assertFalse(isPrivateHost(it), it) }
    }

    @Test
    fun addressesAreNormalised() {
        assertEquals("https://home.example", ok("home.example/"))
        assertEquals("https://home.example:8443/sq", ok(" https://home.example:8443/sq/ "))
        assertEquals("https://nas", ok("https://nas"))
        assertEquals("http://nas.local:8080", ok("http://nas.local:8080"))
        assertEquals("http://192.168.11.218:8080", ok("http://192.168.11.218:8080"))
        assertEquals("http://[fd00::5]:8080", ok("http://[fd00::5]:8080"))
        assertEquals("https://[2001:db8::1]", ok("https://[2001:db8::1]"))
    }

    @Test
    fun badOrUnsafeAddressesAreRefused() {
        assertEquals(INVALID_ADDRESS, refusal("not a url"))
        assertEquals(INVALID_ADDRESS, refusal("https://"))
        assertEquals(INVALID_ADDRESS, refusal("ftp://nas.local"))
        assertEquals(INVALID_ADDRESS, refusal("https://nas.local/?x=1"))
        assertEquals(HTTPS_REQUIRED, refusal("http://squirrel.example.com"))
        assertEquals(HTTPS_REQUIRED, refusal("http://nas"))
    }

    @Test
    fun theClientRefusesCleartextToAPublicHost() = runApiTest {
        var calls = 0
        val api = SquirrelApi("http://squirrel.example.com", MockEngine { calls++; respond("{}") }, Session(InMemoryTokenStore()))
        assertFailsWith<CleartextRefused> { api.health() }
        assertEquals(0, calls)
    }

    @Test
    fun redirectsAreNotFollowed() = runApiTest {
        var calls = 0
        val engine = MockEngine {
            calls++
            respond("", HttpStatusCode.Found, headersOf(HttpHeaders.Location, "http://squirrel.example.com/api/healthz"))
        }
        val api = SquirrelApi("http://192.168.1.2:8080", engine, Session(InMemoryTokenStore()))
        assertEquals(302, assertFailsWith<ApiError> { api.health() }.status)
        assertEquals(1, calls)
    }
}
