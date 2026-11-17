package app.squirrel.shared.api

import io.ktor.client.plugins.auth.AuthProvider
import io.ktor.client.request.HttpRequestBuilder
import io.ktor.client.statement.HttpResponse
import io.ktor.http.Url
import io.ktor.http.auth.HttpAuthHeader

/**
 * Keeps the device's token to its own Squirrel: [delegate] adds it (up front, or on a retry after
 * a 401) only to requests [ours] accepts, and a 401 from anywhere else never refreshes it.
 */
internal class OwnHostOnly(private val delegate: AuthProvider, private val ours: (Url) -> Boolean) : AuthProvider {
    @Deprecated("Use sendWithoutRequest(request)", level = DeprecationLevel.ERROR)
    override val sendWithoutRequest: Boolean get() = false

    override fun sendWithoutRequest(request: HttpRequestBuilder): Boolean = ours(request.url.build())

    override fun isApplicable(auth: HttpAuthHeader): Boolean = delegate.isApplicable(auth)

    override suspend fun addRequestHeaders(request: HttpRequestBuilder, authHeader: HttpAuthHeader?) {
        if (ours(request.url.build())) delegate.addRequestHeaders(request, authHeader)
    }

    override suspend fun refreshToken(response: HttpResponse): Boolean =
        ours(response.call.request.url) && delegate.refreshToken(response)

    override fun clearToken() = delegate.clearToken()
}
