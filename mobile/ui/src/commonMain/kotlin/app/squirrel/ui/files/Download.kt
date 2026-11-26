package app.squirrel.ui.files

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.throwUnlessSuccess
import io.ktor.client.plugins.HttpTimeoutConfig
import io.ktor.client.plugins.timeout
import io.ktor.client.request.prepareGet
import io.ktor.client.statement.bodyAsChannel
import io.ktor.http.contentLength
import io.ktor.utils.io.readAvailable

private const val BUFFER = 64 * 1024

/**
 * Streams `GET url` (an original, a zip) through the signed-in client into [write], reporting
 * progress (0–1) against the response's Content-Length. Throws [app.squirrel.shared.api.ApiError]
 * on an error status.
 */
suspend fun streamDownload(api: SquirrelApi, url: String, onProgress: (Float) -> Unit, write: (ByteArray, Int) -> Unit) {
    api.httpClient.prepareGet(url) {
        timeout { requestTimeoutMillis = HttpTimeoutConfig.INFINITE_TIMEOUT_MS } // large originals and zips
    }.execute { response ->
        response.throwUnlessSuccess()
        val total = response.contentLength()
        val channel = response.bodyAsChannel()
        val buffer = ByteArray(BUFFER)
        var done = 0L
        while (true) {
            val n = channel.readAvailable(buffer, 0, buffer.size)
            if (n < 0) break
            if (n == 0) {
                if (channel.isClosedForRead) break
                continue
            }
            write(buffer, n)
            done += n
            if (total != null && total > 0) onProgress((done.toFloat() / total).coerceAtMost(1f))
        }
    }
}
