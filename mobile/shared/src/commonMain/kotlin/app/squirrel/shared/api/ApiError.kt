package app.squirrel.shared.api

import app.squirrel.api.models.ErrorBody
import io.ktor.client.call.body
import io.ktor.client.statement.HttpResponse
import io.ktor.http.isSuccess
import kotlinx.coroutines.CancellationException
import app.squirrel.api.infrastructure.HttpResponse as GeneratedResponse

/** A non-2xx answer, from the server's `{error: {code, message}}` envelope when it has one. */
open class ApiError(val status: Int, val code: String, message: String) : Exception(message)

/** The device's tokens were refused and could not be refreshed: sign in again. */
class SignedOut : ApiError(401, "unauthorized", "Signed out")

internal const val UNAUTHORIZED = "unauthorized"

/** Builds the [ApiError] for a failed response. */
suspend fun HttpResponse.apiError(): ApiError {
    val detail = try {
        body<ErrorBody>().error
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }
    val code = detail?.code ?: "http_${status.value}"
    if (status.value == 401 && code == UNAUTHORIZED) return SignedOut()
    return ApiError(status.value, code, detail?.message ?: status.description)
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
