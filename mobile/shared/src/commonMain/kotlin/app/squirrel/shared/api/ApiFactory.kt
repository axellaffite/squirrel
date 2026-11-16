package app.squirrel.shared.api

import app.squirrel.api.apis.AlbumsApi
import app.squirrel.api.apis.DevicesApi
import app.squirrel.api.apis.FilesApi
import app.squirrel.api.apis.PhotosApi
import app.squirrel.api.apis.SetupApi
import app.squirrel.api.apis.StorageApi
import app.squirrel.api.apis.SyncApi
import app.squirrel.api.apis.SystemApi
import app.squirrel.api.apis.UploadsApi
import app.squirrel.api.models.AccountDto
import app.squirrel.api.models.DeviceDto
import app.squirrel.api.models.DeviceInfo
import app.squirrel.api.models.DeviceLoginRequest
import app.squirrel.api.models.DevicePlatform
import app.squirrel.api.models.Health
import app.squirrel.api.models.RefreshRequest
import app.squirrel.api.models.SetupStatus
import app.squirrel.shared.auth.RefreshOutcome
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import io.ktor.client.HttpClient
import io.ktor.client.HttpClientConfig
import io.ktor.client.plugins.api.createClientPlugin
import io.ktor.client.engine.HttpClientEngine
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.auth.Auth
import io.ktor.client.plugins.auth.authProvider
import io.ktor.client.plugins.auth.providers.BearerAuthProvider
import io.ktor.client.plugins.auth.providers.BearerTokens
import io.ktor.client.plugins.auth.providers.bearer
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.serialization.kotlinx.json.json
import kotlinx.serialization.json.Json

/** The device as it introduces itself at sign-in. */
data class DeviceDetails(val name: String, val platform: DevicePlatform, val model: String?, val appVersion: String)

/** The result of a successful device sign-in. */
data class SignedIn(val account: AccountDto, val device: DeviceDto)

internal const val REQUEST_TIMEOUT_MS = 30_000L

/** JSON settings shared by every call: tolerant of new server fields, omits nulls. */
val ApiJson: Json = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
}

/**
 * The generated API classes over two Ktor clients: [authed] adds the device's bearer token and
 * refreshes it on a 401 (single-flight, through [Session]); the public client has no auth plugin,
 * so the refresh call itself never recurses.
 */
class SquirrelApi(
    val baseUrl: String,
    engine: HttpClientEngine,
    private val session: Session,
) {
    private val public: HttpClient = HttpClient(engine) { base() }

    private val authed: HttpClient = HttpClient(engine) {
        base()
        install(Auth) {
            bearer {
                loadTokens { session.accessToken()?.let { BearerTokens(it, null) } }
                refreshTokens {
                    session.refresh(oldTokens?.accessToken, ::callRefresh)?.let { BearerTokens(it, null) }
                }
                sendWithoutRequest { true }
            }
        }
    }

    private val publicDevices = DevicesApi(baseUrl, public)
    val system = SystemApi(baseUrl, public)
    val setup = SetupApi(baseUrl, public)
    val devices = DevicesApi(baseUrl, authed)
    val sync = SyncApi(baseUrl, authed)
    val uploadsApi = UploadsApi(baseUrl, authed)
    val photos = PhotosApi(baseUrl, authed)
    val files = FilesApi(baseUrl, authed)
    val albums = AlbumsApi(baseUrl, authed)
    val storage = StorageApi(baseUrl, authed)
    val uploads = UploadsFacade(baseUrl, authed)
    val chunks = ChunkUploader(baseUrl, authed, public)

    suspend fun health(): Health = system.healthz().bodyOrThrow()

    suspend fun setupStatus(): SetupStatus = setup.setupStatus().bodyOrThrow()

    /** `POST /api/auth/device/login`; on success the session holds the new tokens. */
    suspend fun signIn(username: String, password: String, device: DeviceDetails): SignedIn {
        val request = DeviceLoginRequest(
            username = username,
            password = password,
            device = DeviceInfo(
                name = device.name,
                platform = device.platform,
                model = device.model,
                appVersion = device.appVersion,
            ),
        )
        val response = publicDevices.deviceLogin(request).bodyOrThrow()
        session.signedIn(Tokens(response.tokens.accessToken, response.tokens.refreshToken))
        resetCachedTokens()
        return SignedIn(response.account, response.device)
    }

    /** Revokes this device on the server (best effort) and forgets the tokens. */
    suspend fun signOut() {
        try {
            devices.deviceLogout()
        } finally {
            session.signOut()
            resetCachedTokens()
        }
    }

    private suspend fun callRefresh(refreshToken: String): RefreshOutcome {
        val response = publicDevices.refreshDeviceToken(RefreshRequest(refreshToken))
        if (response.status == 401) return RefreshOutcome.Rejected
        val tokens = response.bodyOrThrow().tokens
        return RefreshOutcome.Refreshed(Tokens(tokens.accessToken, tokens.refreshToken))
    }

    /** Ktor caches what `loadTokens` returned; a new sign-in must not reuse it. */
    private fun resetCachedTokens() {
        authed.authProvider<BearerAuthProvider>()?.clearToken()
    }

    fun close() {
        authed.close()
        public.close()
    }
}

/** Refuses plain HTTP to public hosts on every request (the address check alone is not enough). */
private val CleartextGuard = createClientPlugin("CleartextGuard") {
    onRequest { request, _ ->
        val url = request.url.build()
        if (!cleartextAllowed(url)) throw CleartextRefused(url.host)
    }
}

private fun HttpClientConfig<*>.base() {
    expectSuccess = false
    // The API never redirects; following one could leave the allowed hosts.
    followRedirects = false
    install(CleartextGuard)
    install(ContentNegotiation) { json(ApiJson) }
    install(HttpTimeout) {
        requestTimeoutMillis = REQUEST_TIMEOUT_MS
        connectTimeoutMillis = REQUEST_TIMEOUT_MS
        socketTimeoutMillis = REQUEST_TIMEOUT_MS
    }
}
