package app.squirrel.ui.signin

import app.squirrel.api.models.DevicePlatform
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
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
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.flow.first
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull

@OptIn(ExperimentalCoroutinesApi::class)
class SignInViewModelTest {
    private val device = DeviceDetails("Pixel 8", DevicePlatform.android, "Pixel 8", "0.1.0")
    private val connected = mutableListOf<String>()

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun MockRequestHandleScope.json(body: String, status: HttpStatusCode = HttpStatusCode.OK) =
        respond(body, status, headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString()))

    private fun viewModel(
        handler: suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData,
    ): SignInViewModel {
        val session = Session(InMemoryTokenStore())
        return SignInViewModel(
            apiFor = { url -> connected += url; SquirrelApi(url, MockEngine(handler), session) },
            device = device,
        )
    }

    private fun MockRequestHandleScope.server(request: HttpRequestData, login: HttpResponseData? = null) =
        when (request.url.encodedPath) {
            "/api/healthz" -> json("""{"status":"ok"}""")
            "/api/setup/status" -> json("""{"needs_setup":false}""")
            else -> login ?: json(LOGIN, HttpStatusCode.Created)
        }

    /** Actions set `busy` before returning, so the next idle state is their outcome. */
    private suspend fun SignInViewModel.settle(): SignInState =
        withTimeout(5_000) { state.first { !it.busy } }

    private fun runVm(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    @Test
    fun theLastAddressIsPrefilled() {
        val vm = SignInViewModel(apiFor = { error("unused") }, device = device, lastServer = "http://192.168.11.218:8080")
        assertEquals("http://192.168.11.218:8080", vm.state.value.server)
    }

    @Test
    fun anInvalidAddressIsAFieldError() = runVm {
        val vm = viewModel { error("no request expected") }
        vm.onServerChange("not a url")
        vm.onContinue()
        assertEquals("Enter an address like https://squirrel.example.com", vm.state.value.serverError)
        assertEquals(SignInStep.Server, vm.state.value.step)
    }

    @Test
    fun plainHttpIsRefusedForAPublicAddress() = runVm {
        val vm = viewModel { error("no request expected") }
        vm.onServerChange("http://squirrel.example.com")
        vm.onContinue()
        assertEquals("Use https:// for an address outside your home network or VPN", vm.state.value.serverError)
    }

    @Test
    fun plainHttpOnTheHomeNetworkIsAcceptedAndFlagged() = runVm {
        val vm = viewModel { server(it) }
        vm.onServerChange("http://192.168.11.218:8080")
        vm.onContinue()
        val state = vm.settle()
        assertEquals(SignInStep.Credentials, state.step)
        assertEquals(true, state.cleartext)
    }

    @Test
    fun noAnswerShowsAnAlert() = runVm {
        val vm = viewModel { respondError(HttpStatusCode.BadGateway) }
        vm.onServerChange("home.example")
        run {
            vm.onContinue()
            val state = vm.settle()
            assertEquals("No Squirrel answered at this address", state.alert)
            assertEquals(SignInStep.Server, state.step)
        }
    }

    @Test
    fun aServerThatNeedsSetupIsExplained() = runVm {
        val vm = viewModel { request ->
            if (request.url.encodedPath == "/api/setup/status") json("""{"needs_setup":true}""") else server(request)
        }
        vm.onServerChange("home.example")
        run {
            vm.onContinue()
            assertEquals("This Squirrel is not set up yet: finish the setup in a browser first", vm.settle().alert)
        }
    }

    @Test
    fun aGoodServerThenGoodCredentialsSignIn() = runVm {
        val vm = viewModel { server(it) }
        vm.onServerChange("home.example")
        run {
            vm.onContinue()
            assertEquals(SignInStep.Credentials, vm.settle().step)
            vm.onUsernameChange("ada")
            vm.onPasswordChange("secret")
            vm.onSignIn()
            val done = vm.settle()
            val signedIn = assertIs<SignInStep.SignedIn>(done.step)
            assertEquals("Ada", signedIn.result.account.displayName)
            assertEquals("https://home.example", signedIn.serverUrl)
            assertEquals("", done.password)
        }
        assertEquals(listOf("https://home.example"), connected.distinct())
    }

    @Test
    fun wrongCredentialsAreSaidPlainly() = runVm {
        val vm = viewModel {
            server(it, json("""{"error":{"code":"invalid_credentials","message":"x"}}""", HttpStatusCode.Unauthorized))
        }
        run {
            signInWith(vm)
            val state = vm.settle()
            assertEquals("Wrong username or password", state.alert)
            assertEquals(SignInStep.Credentials, state.step)
        }
    }

    @Test
    fun rateLimitingAsksToWait() = runVm {
        val vm = viewModel {
            server(it, json("""{"error":{"code":"rate_limited","message":"x"}}""", HttpStatusCode.TooManyRequests))
        }
        run {
            signInWith(vm)
            assertEquals("Too many attempts. Try again in a minute.", vm.settle().alert)
        }
    }

    @Test
    fun emptyCredentialsAreFieldErrors() = runVm {
        val vm = viewModel { server(it) }
        run {
            vm.onServerChange("home.example")
            vm.onContinue()
            vm.settle()
            vm.onSignIn()
            val state = vm.state.value
            assertEquals("Enter your username", state.usernameError)
            assertEquals("Enter your password", state.passwordError)
            assertNull(state.alert)
        }
    }

    private suspend fun signInWith(vm: SignInViewModel) {
        vm.onServerChange("home.example")
        vm.onContinue()
        vm.settle()
        vm.onUsernameChange("ada")
        vm.onPasswordChange("nope")
        vm.onSignIn()
    }

    private companion object {
        const val LOGIN =
            """{"device":{"id":"d","name":"Pixel 8","platform":"android","model":"Pixel 8","app_version":"0.1.0",""" +
                """"created_at":"2026-11-16T12:00:00Z","last_seen_at":"2026-11-16T12:00:00Z","revoked_at":null,"current":true},""" +
                """"account":{"id":"a","username":"ada","display_name":"Ada","role":"user"},""" +
                """"tokens":{"access_token":"sqa_1","access_expires_at":"2026-11-16T12:15:00Z",""" +
                """"refresh_token":"sqr_1","refresh_expires_at":"2027-02-14T12:00:00Z"}}"""
    }
}
