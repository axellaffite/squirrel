package app.squirrel.ui.signin

import app.squirrel.api.models.DevicePlatform
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.toByteArray
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.milliseconds

/** The sign-in screens driven the way an agent does: ls, then do, through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class SignInLsDoTest {
    private val device = DeviceDetails("Pixel 8", DevicePlatform.android, "Pixel 8", "0.1.0")

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    /** A set-up Squirrel that accepts ada / [PASSWORD] and nothing else. */
    private fun viewModel(): SignInViewModel {
        val session = Session(InMemoryTokenStore())
        val json = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())
        val engine = MockEngine { request ->
            when (request.url.encodedPath) {
                "/api/healthz" -> respond("""{"status":"ok"}""", headers = json)
                "/api/setup/status" -> respond("""{"needs_setup":false}""", headers = json)
                else -> {
                    val body = request.body.toByteArray().decodeToString()
                    if (PASSWORD in body) {
                        respond(LOGIN, HttpStatusCode.Created, json)
                    } else {
                        respond("""{"error":{"code":"invalid_credentials","message":"x"}}""", HttpStatusCode.Unauthorized, json)
                    }
                }
            }
        }
        return SignInViewModel(apiFor = { url -> SquirrelApi(url, engine, session) }, device = device)
    }

    private fun registry(vm: SignInViewModel) =
        LsDoRegistry(settle = 50.milliseconds).apply { register("signin", vm.lsdoScreen) }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun anAgentGoesFromAFreshInstallToSignedIn() = run {
        val registry = registry(viewModel())

        assertTrue(registry.perform("signin", intent("""{"type":"set_server","address":"home.example"}""")).ok)
        val connected = registry.perform("signin", intent("""{"type":"continue"}"""))
        assertTrue(connected.ok, connected.message)
        val credentials = connected.screens.screen()
        assertEquals("credentials", credentials.state()["step"]?.jsonPrimitive?.content)
        assertEquals("https://home.example", credentials.state()["server"]?.jsonPrimitive?.content)
        assertEquals(setOf("set_username", "set_password", "sign_in", "change_server"), credentials.actionNames().toSet())

        registry.perform("signin", intent("""{"type":"set_username","username":"ada"}"""))
        registry.perform("signin", intent("""{"type":"set_password","password":"$PASSWORD"}"""))
        val signedIn = registry.perform("signin", intent("""{"type":"sign_in"}"""))

        assertTrue(signedIn.ok, signedIn.message)
        assertEquals("signed_in", signedIn.screens.screen().state()["step"]?.jsonPrimitive?.content)
        assertTrue(signedIn.screens.screen().actionNames().isEmpty())
    }

    @Test
    fun wrongCredentialsAndABadAddressAreRefusals() = run {
        val registry = registry(viewModel())

        val badAddress = registry.perform("signin", intent("""{"type":"continue","address":"http://squirrel.example.com"}"""))
        assertFalse(badAddress.ok)
        assertEquals("Use https:// for an address outside your home network or VPN", badAddress.message)

        registry.perform("signin", intent("""{"type":"continue","address":"home.example"}"""))
        val empty = registry.perform("signin", intent("""{"type":"sign_in"}"""))
        assertEquals("Enter your username. Enter your password", empty.message)

        val wrong = registry.perform("signin", intent("""{"type":"sign_in","username":"ada","password":"nope"}"""))
        assertFalse(wrong.ok)
        assertEquals("Wrong username or password", wrong.message)
        assertEquals("credentials", wrong.screens.screen().state()["step"]?.jsonPrimitive?.content)
    }

    @Test
    fun eachStepOffersOnlyItsOwnActions() = run {
        val registry = registry(viewModel())

        val server = registry.ls("signin") ?: error("signin is listed")
        assertEquals(setOf("set_server", "continue"), server.actionNames().toSet())
        assertTrue(registry.perform("signin", intent("""{"type":"sign_in"}""")).message.orEmpty().contains("not available"))

        registry.perform("signin", intent("""{"type":"continue","address":"home.example"}"""))
        val back = registry.perform("signin", intent("""{"type":"change_server"}"""))
        assertTrue(back.ok, back.message)
        assertEquals("server", back.screens.screen().state()["step"]?.jsonPrimitive?.content)
        assertTrue(registry.perform("signin", intent("""{"type":"set_password","password":"x"}""")).message.orEmpty().contains("not available"))
    }

    @Test
    fun thePasswordAndTokensNeverReachTheAgent() = run {
        val registry = registry(viewModel())
        val listings = mutableListOf<String>()

        registry.perform("signin", intent("""{"type":"continue","address":"home.example"}"""))
        val typed = registry.perform("signin", intent("""{"type":"set_password","password":"$PASSWORD"}"""))
        assertEquals("true", typed.screens.screen().state()["passwordSet"]?.jsonPrimitive?.content)
        listings += typed.toJson().toString()
        listings += registry.perform("signin", intent("""{"type":"sign_in"}""")).toJson().toString()
        listings += registry.perform("signin", intent("""{"type":"sign_in","username":"ada","password":"$PASSWORD"}""")).toJson().toString()
        listings += registry.ls().toString()

        listings.forEach { text ->
            listOf(PASSWORD, "sqa_1", "sqr_1").forEach { secret -> assertFalse(secret in text, "\"$secret\" in $text") }
        }
    }

    private fun JsonObject.screen() = getValue("screens").jsonArray.single().jsonObject

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content }

    private companion object {
        const val PASSWORD = "correct-horse-battery"
        const val LOGIN =
            """{"device":{"id":"d","name":"Pixel 8","platform":"android","model":"Pixel 8","app_version":"0.1.0",""" +
                """"created_at":"2026-11-16T12:00:00Z","last_seen_at":"2026-11-16T12:00:00Z","revoked_at":null,"current":true},""" +
                """"account":{"id":"a","username":"ada","display_name":"Ada","role":"user"},""" +
                """"tokens":{"access_token":"sqa_1","access_expires_at":"2026-11-16T12:15:00Z",""" +
                """"refresh_token":"sqr_1","refresh_expires_at":"2027-02-14T12:00:00Z"}}"""
    }
}
