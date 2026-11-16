package app.squirrel.ui.signin

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.api.SignedIn
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.isCleartext
import app.squirrel.shared.api.normalizeServerUrl
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

sealed interface SignInStep {
    data object Server : SignInStep
    data object Credentials : SignInStep
    data class SignedIn(val serverUrl: String, val api: SquirrelApi, val result: app.squirrel.shared.api.SignedIn) : SignInStep
}

data class SignInState(
    val step: SignInStep = SignInStep.Server,
    val server: String = "https://",
    val serverError: String? = null,
    val username: String = "",
    val usernameError: String? = null,
    val password: String = "",
    val passwordError: String? = null,
    val alert: String? = null,
    val busy: Boolean = false,
) {
    /** Plain HTTP to a home or VPN address: the screen says it is not encrypted. */
    val cleartext: Boolean get() = step != SignInStep.Server && isCleartext(server)
}

private const val NO_ANSWER = "No Squirrel answered at this address"
private const val NEEDS_SETUP = "This Squirrel is not set up yet: finish the setup in a browser first"
private const val WRONG_CREDENTIALS = "Wrong username or password"
private const val RATE_LIMITED = "Too many attempts. Try again in a minute."
private const val UNREACHABLE = "Squirrel could not be reached. Check your connection and try again."

/** The two sign-in screens: the server address, then username and password. */
class SignInViewModel(
    private val apiFor: (serverUrl: String) -> SquirrelApi,
    private val device: DeviceDetails,
    /** The address used before, prefilled after a sign-out or revocation. */
    lastServer: String? = null,
) : ViewModel() {
    private val _state = MutableStateFlow(SignInState(server = lastServer ?: "https://"))
    val state: StateFlow<SignInState> = _state.asStateFlow()
    private var api: SquirrelApi? = null
    private var serverUrl: String? = null

    fun onServerChange(value: String) = _state.update { it.copy(server = value, serverError = null, alert = null) }
    fun onUsernameChange(value: String) = _state.update { it.copy(username = value, usernameError = null, alert = null) }
    fun onPasswordChange(value: String) = _state.update { it.copy(password = value, passwordError = null, alert = null) }

    /** Back from the credentials to the address. */
    fun onChangeServer() = _state.update { it.copy(step = SignInStep.Server, alert = null, password = "") }

    fun onContinue() {
        if (_state.value.busy) return
        val url = normalizeServerUrl(_state.value.server).getOrElse { error ->
            _state.update { it.copy(serverError = error.message) }
            return
        }
        _state.update { it.copy(busy = true, alert = null, serverError = null) }
        viewModelScope.launch {
            val candidate = apiFor(url)
            val alert = probe(candidate)
            if (alert == null) {
                api?.takeIf { it !== candidate }?.close()
                api = candidate
                serverUrl = url
                _state.update { it.copy(busy = false, step = SignInStep.Credentials, server = url) }
            } else {
                candidate.close()
                _state.update { it.copy(busy = false, alert = alert) }
            }
        }
    }

    fun onSignIn() {
        val current = _state.value
        if (current.busy) return
        val usernameError = if (current.username.isBlank()) "Enter your username" else null
        val passwordError = if (current.password.isEmpty()) "Enter your password" else null
        if (usernameError != null || passwordError != null) {
            _state.update { it.copy(usernameError = usernameError, passwordError = passwordError) }
            return
        }
        val api = this.api ?: return onChangeServer()
        val url = serverUrl ?: return onChangeServer()
        _state.update { it.copy(busy = true, alert = null) }
        viewModelScope.launch {
            try {
                val result: SignedIn = api.signIn(current.username.trim(), current.password, device)
                _state.update { it.copy(busy = false, password = "", step = SignInStep.SignedIn(url, api, result)) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiError) {
                _state.update { it.copy(busy = false, alert = signInMessage(e)) }
            } catch (_: Exception) {
                _state.update { it.copy(busy = false, alert = UNREACHABLE) }
            }
        }
    }

    /** Null when the address answers as a set-up Squirrel, else the alert to show. */
    private suspend fun probe(api: SquirrelApi): String? = try {
        api.health()
        if (api.setupStatus().needsSetup) NEEDS_SETUP else null
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        NO_ANSWER
    }

    private fun signInMessage(error: ApiError): String = when (error.code) {
        "invalid_credentials" -> WRONG_CREDENTIALS
        "rate_limited" -> RATE_LIMITED
        else -> error.message ?: UNREACHABLE
    }
}
