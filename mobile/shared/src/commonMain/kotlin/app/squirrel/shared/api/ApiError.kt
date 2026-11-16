package app.squirrel.shared.api

import app.squirrel.api.models.ErrorBody
import app.squirrel.shared.auth.Session
import io.ktor.client.call.body
import io.ktor.client.statement.HttpResponse
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.contentType
import io.ktor.http.isSuccess
import io.ktor.util.AttributeKey
import kotlinx.coroutines.CancellationException
import app.squirrel.api.infrastructure.HttpResponse as GeneratedResponse

/**
 * A non-2xx answer. [fromServer]: the body was Squirrel's `{error: {code, message}}` envelope
 * (not, say, a proxy's HTML page).
 */
open class ApiError(
    val status: Int,
    val code: String,
    message: String,
    val fromServer: Boolean = true,
    /** The `Retry-After` delay in seconds (429, 503), when the server gave one. */
    val retryAfterSeconds: Long? = null,
) : Exception(message)

/** The server refused the device's tokens and they could not be refreshed: sign in again. */
class SignedOut : ApiError(401, "unauthorized", "Signed out")

/** Set on requests made with the device's bearer token (the authenticated client). */
internal val AuthenticatedCall = AttributeKey<Session>("SquirrelAuthenticatedCall")

/** Whether a response is the server's own JSON (errors carry the envelope). */
internal fun HttpResponse.isServerJson(): Boolean =
    contentType()?.match(ContentType.Application.Json) == true

/**
 * Builds the [ApiError] for a failed response. A 401 from the server on an authenticated call,
 * whatever its code, signs out and is [SignedOut]; a 401 without the envelope stays an ordinary,
 * retryable error.
 */
suspend fun HttpResponse.apiError(): ApiError {
    val detail = if (isServerJson()) {
        try {
            body<ErrorBody>().error
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
    } else {
        null
    }
    val session = call.request.attributes.getOrNull(AuthenticatedCall)
    // The server's final word after the Auth plugin's refresh and retry: signed out, unless
    // the refresh itself failed for a transient reason (then this 401 is retried later).
    if (status.value == 401 && detail != null && session != null && session.refused()) return SignedOut()
    return ApiError(
        status = status.value,
        code = detail?.code ?: "http_${status.value}",
        message = detail?.message ?: status.description,
        fromServer = detail != null,
        retryAfterSeconds = headers[HttpHeaders.RetryAfter]?.trim()?.toLongOrNull()?.takeIf { it >= 0 },
    )
}

/** The decoded body of a generated call, or its [ApiError]. */
suspend fun <T : Any> GeneratedResponse<T>.bodyOrThrow(): T {
    if (!success) throw response.apiError()
    return body()
}

/** Throws the [ApiError] of a failed raw response. */
suspend fun HttpResponse.throwUnlessSuccess(): HttpResponse {
    if (!status.isSuccess()) throw apiError()
    return this
}
