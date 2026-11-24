package app.squirrel.shared.api

import app.squirrel.api.models.SyncCheckItem
import app.squirrel.api.models.SyncCheckRequest
import app.squirrel.api.apis.AlbumsApi
import app.squirrel.api.apis.CommentsApi
import app.squirrel.api.apis.DevicesApi
import app.squirrel.api.apis.ExploreApi
import app.squirrel.api.apis.FilesApi
import app.squirrel.api.apis.NotificationsApi
import app.squirrel.api.apis.PeopleApi
import app.squirrel.api.apis.SearchApi
import app.squirrel.api.apis.SharingApi
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
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.http.Url
import io.ktor.http.encodeURLPathPart
import io.ktor.serialization.kotlinx.json.json
import kotlinx.serialization.json.Json

/** The device as it introduces itself at sign-in. */
data class DeviceDetails(val name: String, val platform: DevicePlatform, val model: String?, val appVersion: String)

/** The result of a successful device sign-in. */
data class SignedIn(val account: AccountDto, val device: DeviceDto)

internal const val REQUEST_TIMEOUT_MS = 30_000L
private const val SYNC_CHECK_BATCH = 500
private val REFUSED = setOf(400, 401, 403)

/**
 * JSON settings shared by every call: tolerant of new server fields, omits nulls. Defaults are
 * written: a required list defaults to empty in the generated models (for older servers), and an
 * empty one must still be sent.
 */
val ApiJson: Json = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
    encodeDefaults = true
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
    private val base = Url(baseUrl)

    /** Same scheme, host and port as [baseUrl]. */
    private fun ours(url: Url) = url.protocol == base.protocol && url.host == base.host && url.port == base.port

    private val public: HttpClient = HttpClient(engine) { base() }

    private val authed: HttpClient = HttpClient(engine) {
        base()
        install(createClientPlugin("SquirrelSession") {
            onRequest { request, _ -> request.attributes.put(AuthenticatedCall, session) }
        })
        install(Auth) {
            val bearer = BearerAuthProvider(
                refreshTokens = {
                    // A 401 from another host never spends this device's refresh token.
                    if (!ours(response.call.request.url)) return@BearerAuthProvider null
                    session.refresh(oldTokens?.accessToken, ::callRefresh)?.let { BearerTokens(it, null) }
                },
                loadTokens = { session.accessToken()?.let { BearerTokens(it, null) } },
                sendWithoutRequestCallback = { true },
                realm = null,
            )
            providers += OwnHostOnly(bearer) { ours(it) }
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
    val search = SearchApi(baseUrl, authed)
    val explore = ExploreApi(baseUrl, authed)

    /** Read-only on the phone: names and avatars for filters (naming and merging stay on the web). */
    val people = PeopleApi(baseUrl, authed)
    val sharing = SharingApi(baseUrl, authed)
    val comments = CommentsApi(baseUrl, authed)
    val notifications = NotificationsApi(baseUrl, authed)
    val uploads = UploadsFacade(baseUrl, authed)
    val chunks = ChunkUploader(baseUrl, authed, public)

    /** The authenticated client, for image loading and downloads (bearer and refresh included). */
    val httpClient: HttpClient get() = authed

    /**
     * `GET /api/files/{id}/thumbnail`; [version] makes the URL change whenever it is regenerated.
     * [px] (previews only, see [PreviewSize]): the long side wanted, in device pixels; without it, 1440.
     */
    fun thumbnailUrl(id: String, version: Int, preview: Boolean = false, px: Int? = null): String {
        val size = if (preview) "preview" else "thumb"
        val sized = px?.let { "&px=$it" }.orEmpty()
        return "$baseUrl/api/files/${id.encodeURLPathPart()}/thumbnail?size=$size&v=$version$sized"
    }

    /** `GET /api/files/{id}/content`: the original bytes. */
    fun contentUrl(id: String): String = "$baseUrl/api/files/${id.encodeURLPathPart()}/content"

    suspend fun health(): Health = system.healthz().bodyOrThrow()

    suspend fun setupStatus(): SetupStatus = setup.setupStatus().bodyOrThrow()

    /** `POST /api/auth/device/login`; on success the session holds the new tokens. */
    @Throws(Exception::class)
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

    /**
     * Which of these assets (device asset id to SHA-256) the server does not have yet, through
     * `POST /api/sync/check` in batches of 500. Also for Swift callers (the end-to-end test).
     */
    @Throws(Exception::class)
    suspend fun missingOnServer(hashes: Map<String, String>): List<String> =
        hashes.entries.chunked(SYNC_CHECK_BATCH).flatMap { batch ->
            sync.syncCheck(SyncCheckRequest(batch.map { SyncCheckItem(it.key, it.value) })).bodyOrThrow().missing
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

    /**
     * The server answers 401 for a revoked device, an unknown, expired or reused token and a
     * disabled user, and 400 for a malformed one: any of 400/401/403 in its own envelope means
     * sign in again. Without the envelope (a proxy page) it is a transient failure.
     */
    private suspend fun callRefresh(refreshToken: String): RefreshOutcome {
        val response = publicDevices.refreshDeviceToken(RefreshRequest(refreshToken))
        if (response.status in REFUSED) {
            val error = response.response.apiError()
            if (error.fromServer) return RefreshOutcome.Rejected
            throw error
        }
        val tokens = response.bodyOrThrow().tokens
        return RefreshOutcome.Refreshed(Tokens(tokens.accessToken, tokens.refreshToken))
    }

    /** Ktor caches what `loadTokens` returned; a new sign-in must not reuse it. */
    private fun resetCachedTokens() {
        authed.authProvider<OwnHostOnly>()?.clearToken()
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
