package app.squirrel.ui

import app.cash.sqldelight.db.SqlDriver
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.api.SignedIn
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.SessionState
import app.squirrel.shared.auth.TokenStore
import app.squirrel.shared.db.SquirrelDatabase
import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.db.forgetDevice
import app.squirrel.shared.settings.SettingsStore
import io.ktor.client.engine.HttpClientEngine
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Who is signed in, as the shell shows it (cached so the app starts offline). */
data class AccountInfo(val serverUrl: String, val displayName: String, val username: String, val deviceName: String)

sealed interface AppState {
    data object Loading : AppState
    /** [attempt] changes on every sign-out so the sign-in screens start fresh, at [serverUrl]. */
    data class NeedsSignIn(val attempt: Int, val serverUrl: String? = null) : AppState
    data class SignedIn(val account: AccountInfo) : AppState
}

private object AccountKeys {
    const val DISPLAY_NAME = "account.display_name"
    const val USERNAME = "account.username"
    const val DEVICE_NAME = "device.name"
}

/** Manual wiring of the shared pieces; one per process, built by each platform's entry point. */
class AppGraph(
    driver: SqlDriver,
    tokenStore: (SquirrelDatabase) -> TokenStore,
    private val engine: () -> HttpClientEngine,
    val device: DeviceDetails,
    private val scope: CoroutineScope,
    /** Database and keystore work runs here, never on the main thread. */
    private val io: CoroutineDispatcher = Dispatchers.IO,
) {
    val database: SquirrelDatabase = createDatabase(driver)
    val settings = SettingsStore(database)
    val session = Session(tokenStore(database))
    private val _state = MutableStateFlow<AppState>(AppState.Loading)
    val state: StateFlow<AppState> = _state.asStateFlow()
    private var attempts = 0
    var api: SquirrelApi? = null
        private set

    init {
        scope.launch { start() }
        scope.launch {
            session.state.collect { if (it == SessionState.SignedOut && _state.value is AppState.SignedIn) toSignIn() }
        }
    }

    fun newApi(serverUrl: String): SquirrelApi = SquirrelApi(serverUrl, engine(), session)

    private suspend fun start() {
        val restored = withContext(io) {
            val url = settings.serverUrl()
            val name = settings.value(AccountKeys.DISPLAY_NAME)
            if (session.restore() != SessionState.SignedIn || url == null || name == null) {
                session.signOut()
                null
            } else {
                AccountInfo(
                    serverUrl = url,
                    displayName = name,
                    username = settings.value(AccountKeys.USERNAME).orEmpty(),
                    deviceName = settings.value(AccountKeys.DEVICE_NAME) ?: device.name,
                )
            }
        } ?: return toSignIn()
        api = newApi(restored.serverUrl)
        _state.value = AppState.SignedIn(restored)
        refreshAccount()
    }

    /** After the sign-in screens succeeded. */
    fun onSignedIn(serverUrl: String, api: SquirrelApi, result: SignedIn) {
        scope.launch(io) {
            settings.setServerUrl(serverUrl)
            remember(result)
        }
        this.api = api
        _state.value = AppState.SignedIn(info(serverUrl, result))
    }

    /** Re-reads the account and device; a revoked device ends up signed out. */
    fun refreshAccount() {
        val api = api ?: return
        scope.launch {
            try {
                val current = api.devices.getCurrentDevice().bodyOrThrow()
                val result = SignedIn(current.account, current.device)
                withContext(io) { remember(result) }
                _state.update { if (it is AppState.SignedIn) AppState.SignedIn(info(api.baseUrl, result)) else it }
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // Offline or signed out: the session's state drives what happens next.
            }
        }
    }

    fun signOut() {
        val api = api
        scope.launch {
            try {
                api?.signOut()
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                session.signOut() // Unreachable server: forget the tokens anyway.
            }
            toSignIn()
        }
    }

    private suspend fun toSignIn() {
        api?.close()
        api = null
        val lastServer = withContext(io) {
            database.forgetDevice()
            settings.serverUrl()
        }
        _state.value = AppState.NeedsSignIn(++attempts, lastServer)
    }

    private fun remember(result: SignedIn) {
        settings.setValue(AccountKeys.DISPLAY_NAME, result.account.displayName)
        settings.setValue(AccountKeys.USERNAME, result.account.username)
        settings.setValue(AccountKeys.DEVICE_NAME, result.device.name)
    }

    private fun info(serverUrl: String, result: SignedIn) = AccountInfo(
        serverUrl = serverUrl,
        displayName = result.account.displayName,
        username = result.account.username,
        deviceName = result.device.name,
    )
}
