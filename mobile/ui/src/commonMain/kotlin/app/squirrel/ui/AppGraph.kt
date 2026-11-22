package app.squirrel.ui

import app.cash.sqldelight.db.SqlDriver
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.api.SignedIn
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.PendingClear
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
import app.squirrel.shared.sync.BackupStatus
import app.squirrel.shared.sync.KtorChunkTransport
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.shared.sync.NetworkWaiter
import app.squirrel.shared.sync.Queue
import app.squirrel.shared.sync.SCAN_ACCESS_KEY
import app.squirrel.shared.sync.Scanner
import app.squirrel.shared.sync.SyncEngine
import app.squirrel.shared.sync.SyncState
import app.squirrel.shared.sync.backupStatus
import app.squirrel.ui.backup.BackupPlatform
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.images.ImagesPlatform
import app.squirrel.ui.images.forgetEverything
import app.squirrel.ui.images.squirrelImageLoader
import coil3.ImageLoader
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOn
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
    /** [scope] holds this session's view models; it is cleared at sign-out. */
    data class SignedIn(val account: AccountInfo, val scope: SessionScope) : AppState
}

private object AccountKeys {
    const val DISPLAY_NAME = "account.display_name"
    const val USERNAME = "account.username"
    const val DEVICE_NAME = "device.name"
}

/** The sign-out that could not delete the stored token, kept in the settings table. */
private class SettingsPendingClear(private val settings: SettingsStore) : PendingClear {
    override suspend fun isPending(): Boolean = settings.value(PENDING_CLEAR_KEY) != null
    override suspend fun set(pending: Boolean) = settings.setValue(PENDING_CLEAR_KEY, if (pending) "1" else null)
}

/** Kept by `forgetDevice` (Setting.sq `removeDeviceState`). */
const val PENDING_CLEAR_KEY = "session.clear_pending"

/** Manual wiring of the shared pieces; one per process, built by each platform's entry point. */
class AppGraph(
    driver: SqlDriver,
    tokenStore: (SquirrelDatabase) -> TokenStore,
    private val httpEngine: () -> HttpClientEngine,
    val device: DeviceDetails,
    private val scope: CoroutineScope,
    /** Database and keystore work runs here, never on the main thread. */
    private val io: CoroutineDispatcher = Dispatchers.IO,
    /** The platform's photo library and scheduler; null where backup is not wired (tests). */
    val backup: BackupPlatform? = null,
    /** Image caches; null where images are not wired. */
    private val images: ImagesPlatform? = null,
    /** Downloads handed to other apps; null where not wired. */
    val files: FileHandoff? = null,
    private val clock: () -> Long = { kotlin.time.Clock.System.now().toEpochMilliseconds() },
) {
    val database: SquirrelDatabase = createDatabase(driver)
    val settings = SettingsStore(database)
    val session = Session(tokenStore(database), SettingsPendingClear(settings))
    private val _state = MutableStateFlow<AppState>(AppState.Loading)
    val state: StateFlow<AppState> = _state.asStateFlow()
    private var attempts = 0
    var api: SquirrelApi? = null
        private set
    val queue = Queue(database)
    private val _syncEngine = MutableStateFlow<SyncEngine?>(null)
    private val _imageLoader = MutableStateFlow<ImageLoader?>(null)

    /** Thumbnails and previews of the signed-in account (null when signed out). */
    val imageLoader: StateFlow<ImageLoader?> = _imageLoader.asStateFlow()

    /** The backup engine of the signed-in device (null when signed out or not wired). */
    val syncEngine: StateFlow<SyncEngine?> = _syncEngine.asStateFlow()

    init {
        scope.launch { start() }
        scope.launch {
            session.state.collect { if (it == SessionState.SignedOut && _state.value is AppState.SignedIn) toSignIn() }
        }
    }

    fun newApi(serverUrl: String): SquirrelApi = SquirrelApi(serverUrl, httpEngine(), session)

    /** Waits until start-up decided between signed in and sign-in. */
    @Throws(Exception::class)
    suspend fun awaitReady(): AppState = state.first { it != AppState.Loading }

    /** After [signOut]: suspends until the server, the token store and the caches are done with. */
    @Throws(Exception::class)
    suspend fun awaitSignedOut(): AppState = state.first { it is AppState.NeedsSignIn }

    /** The live backup status for the screens. */
    @OptIn(ExperimentalCoroutinesApi::class)
    fun backupStatus(): Flow<BackupStatus> = syncEngine.flatMapLatest { engine ->
        backupStatus(queue, engine?.state ?: MutableStateFlow(SyncState.Idle), settings) {
            backup?.source?.access() ?: MediaAccess.Full
        }
    }.flowOn(io)

    /** Re-applies the background schedule after a settings change. */
    fun applySchedule() {
        val platform = backup ?: return
        if (_syncEngine.value == null) return
        scope.launch(io) { platform.schedule(settings.backup()) }
    }

    /**
     * "Back up now": expedited background work, in the foreground, so it carries on when the user
     * leaves the app; the screens follow it through [syncEngine]'s state (same process).
     */
    fun backUpNow() {
        val platform = backup ?: return
        scope.launch(io) {
            queue.retryFailed()
            platform.backUpNow(settings.backup())
        }
    }

    private fun use(api: SquirrelApi) {
        this.api = api
        images?.let { _imageLoader.value = squirrelImageLoader(it, api.httpClient) }
        val platform = backup ?: return
        _syncEngine.value = SyncEngine(
            source = platform.source,
            api = api,
            queue = queue,
            scanner = Scanner(platform.source, database, queue, settings, now = clock),
            transport = platform.transport(api) ?: KtorChunkTransport(api.chunks),
            conditions = platform.conditions,
            settings = settings,
            now = clock,
            io = io,
            network = platform.networkWaiter() ?: NetworkWaiter.Delay,
        )
    }

    /**
     * The app's UI came to the foreground: the place, besides sign-in and settings changes, where
     * the schedule is (re-)applied. A process a worker woke does not touch the schedule.
     */
    fun onAppVisible() {
        applySchedule()
        val platform = backup ?: return
        if (_syncEngine.value == null) return
        scope.launch(io) {
            // Access granted, widened, shrunk or reselected while the app was away: rescan now.
            val seen = settings.value(SCAN_ACCESS_KEY)
            if (seen != null && seen != platform.source.accessFingerprint()) platform.runSoon(settings.backup())
        }
    }

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
        use(newApi(restored.serverUrl))
        _state.value = AppState.SignedIn(restored, newScope())
        refreshAccount()
    }

    /** After the sign-in screens succeeded. */
    fun onSignedIn(serverUrl: String, api: SquirrelApi, result: SignedIn) {
        scope.launch(io) {
            settings.setServerUrl(serverUrl)
            remember(result)
        }
        use(api)
        applySchedule()
        _state.value = AppState.SignedIn(info(serverUrl, result), newScope())
    }

    /** Re-reads the account and device; a revoked device ends up signed out. */
    fun refreshAccount() {
        val api = api ?: return
        scope.launch {
            try {
                val current = api.devices.getCurrentDevice().bodyOrThrow()
                val result = SignedIn(current.account, current.device)
                withContext(io) { remember(result) }
                _state.update { if (it is AppState.SignedIn) it.copy(account = info(api.baseUrl, result)) else it }
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
                try {
                    session.signOut() // Unreachable server: forget the tokens anyway.
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // Signed out in memory regardless; the sign-in screen must still come.
                }
            }
            toSignIn()
        }
    }

    private fun newScope(): SessionScope {
        (_state.value as? AppState.SignedIn)?.scope?.close()
        return SessionScope()
    }

    private suspend fun toSignIn() {
        (_state.value as? AppState.SignedIn)?.scope?.close()
        backup?.cancel()
        // Thumbnails, previews and downloaded files are this account's private data. Screens
        // lose the loader first, so none of them loads into the caches while they are cleared.
        val loader = _imageLoader.value
        _imageLoader.value = null
        loader?.forgetEverything()
        withContext(io) {
            images?.clearFiles()
            files?.clear()
        }
        _syncEngine.value = null
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
