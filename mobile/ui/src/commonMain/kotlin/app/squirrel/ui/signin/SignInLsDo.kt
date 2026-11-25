package app.squirrel.ui.signin

import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import io.github.axellaffite.lsdo.mapState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlin.time.Duration.Companion.seconds

/** Which of the two sign-in screens is shown, as an agent names it. */
@Serializable
enum class SignInStepName {
    @SerialName("server") Server,
    @SerialName("credentials") Credentials,
    @SerialName("signed_in") SignedIn,
}

/**
 * The sign-in screens as an agent sees them. Never the password nor the tokens: [SignInState]
 * holds both (and the API client), so this is a separate view of it.
 */
@Serializable
data class SignInView(
    @Doc("server: the address screen; credentials: username and password; signed_in: done, the app opens") val step: SignInStepName,
    @Doc("The address typed, normalized (https://host) once accepted") val server: String,
    @Doc("Why the address was refused") val serverError: String? = null,
    val username: String = "",
    val usernameError: String? = null,
    @Doc("A password is typed (never shown)") val passwordSet: Boolean = false,
    val passwordError: String? = null,
    @Doc("The failure shown above the button: no answer, not set up, wrong credentials, too many attempts") val alert: String? = null,
    @Doc("A request is in flight; every field and button is disabled") val busy: Boolean = false,
    @Doc("Plain http on a home or VPN address: the screen warns it is not encrypted") val cleartext: Boolean = false,
)

internal fun SignInState.view() = SignInView(
    step = when (step) {
        SignInStep.Server -> SignInStepName.Server
        SignInStep.Credentials -> SignInStepName.Credentials
        is SignInStep.SignedIn -> SignInStepName.SignedIn
    },
    server = server,
    serverError = serverError,
    username = username,
    usernameError = usernameError,
    passwordSet = password.isNotEmpty(),
    passwordError = passwordError,
    alert = alert,
    busy = busy,
    cleartext = cleartext,
)

/** What an agent can do on the sign-in screens (LS-DO); each maps to the [SignInViewModel] call the UI makes. */
@Serializable
sealed interface SignInIntent {
    @Serializable @SerialName("set_server") @Doc("Type the server address (\"home.example\" or \"http://192.168.1.10:8080\"). On the address screen")
    data class SetServer(val address: String) : SignInIntent

    @Serializable @SerialName("continue") @Doc("Check the address and go on to username and password, as the Continue button does. On the address screen")
    data class Continue(@Doc("Type this address first") val address: String? = null) : SignInIntent

    @Serializable @SerialName("set_username") @Doc("Type the username. On the credentials screen")
    data class SetUsername(val username: String) : SignInIntent

    @Serializable @SerialName("set_password") @Doc("Type the password. On the credentials screen")
    data class SetPassword(val password: String) : SignInIntent

    @Serializable @SerialName("sign_in") @Doc("Sign in, as the Sign in button does; the app opens once it succeeds. On the credentials screen")
    data class SignIn(
        @Doc("Type this username first") val username: String? = null,
        @Doc("Type this password first") val password: String? = null,
    ) : SignInIntent

    @Serializable @SerialName("change_server") @Doc("Back to the address screen (clears the password), as the Change link does. On the credentials screen")
    data object ChangeServer : SignInIntent
}

/**
 * The sign-in flow as an LS-DO screen ("signin"). Its state is [SignInView], mapped from the
 * view model's own state so the password and the API client never reach the agent.
 */
class SignInLsDo(private val vm: SignInViewModel, scope: CoroutineScope) : LsDoScreen<SignInView, SignInIntent> {
    override val lsdo = lsdoSpec<SignInView, SignInIntent>()

    override val state: StateFlow<SignInView> =
        vm.state.mapState { it.view() }

    override suspend fun perform(intent: SignInIntent): String? = when (intent) {
        is SignInIntent.SetServer -> done { vm.onServerChange(intent.address) }
        is SignInIntent.Continue -> {
            intent.address?.let(vm::onServerChange)
            vm.onContinue()
            outcome { it.serverError }
        }
        is SignInIntent.SetUsername -> done { vm.onUsernameChange(intent.username) }
        is SignInIntent.SetPassword -> done { vm.onPasswordChange(intent.password) }
        is SignInIntent.SignIn -> {
            intent.username?.let(vm::onUsernameChange)
            intent.password?.let(vm::onPasswordChange)
            vm.onSignIn()
            outcome { listOfNotNull(it.usernameError, it.passwordError).joinToString(". ").ifEmpty { null } }
        }
        SignInIntent.ChangeServer -> done { vm.onChangeServer() }
    }

    /** Field typing on the step shown, nothing while a request runs (the UI disables it all). */
    override fun availableIntents(state: SignInView): Set<String> = when {
        state.busy -> emptySet()
        else -> when (state.step) {
            SignInStepName.Server -> setOf("set_server", "continue")
            SignInStepName.Credentials -> setOf("set_username", "set_password", "sign_in", "change_server")
            SignInStepName.SignedIn -> emptySet()
        }
    }

    override fun isBusy(state: SignInView): Boolean = state.busy

    /**
     * Waits for the request the button started, then refuses with what the screen shows: a field
     * error, or the alert. So a wrong password is a failed do, not a success to read the state of.
     */
    private suspend fun outcome(fieldError: (SignInState) -> String?): String? {
        val settled = withTimeoutOrNull(REQUEST_WAIT) { vm.state.first { !it.busy } } ?: return null
        return fieldError(settled) ?: settled.alert
    }

    private companion object {
        /** Past this the registry answers "still busy" and the agent lists again. */
        val REQUEST_WAIT = 30.seconds
    }
}

/** A call the screen cannot refuse. */
private inline fun done(call: () -> Unit): String? {
    call()
    return null
}
