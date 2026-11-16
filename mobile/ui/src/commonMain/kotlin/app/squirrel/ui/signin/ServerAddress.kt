package app.squirrel.ui.signin

import io.ktor.http.URLProtocol
import io.ktor.http.Url

/** Hosts a debug build may reach over plain HTTP (the emulator's host, this machine). */
private val DEBUG_HTTP_HOSTS = setOf("10.0.2.2", "localhost", "127.0.0.1")

class InvalidAddress(message: String) : Exception(message)

/**
 * Turns what the user typed into `https://host[:port][/path]`: adds `https://`, drops a trailing
 * slash, refuses `http://` except to [DEBUG_HTTP_HOSTS] in debug builds ([allowHttp]).
 */
fun normalizeServerUrl(input: String, allowHttp: Boolean): Result<String> {
    val trimmed = input.trim().trimEnd('/')
    val withScheme = if ("://" in trimmed) trimmed else "https://$trimmed"
    val url = runCatching { Url(withScheme) }.getOrNull()
    val valid = url != null && url.host.isNotBlank() && " " !in trimmed && url.host.contains('.') ||
        url?.host in DEBUG_HTTP_HOSTS
    if (url == null || !valid || url.parameters.names().isNotEmpty() || url.fragment.isNotEmpty()) {
        return Result.failure(InvalidAddress("Enter an address like https://squirrel.example.com"))
    }
    return when (url.protocol) {
        URLProtocol.HTTPS -> Result.success(withScheme)
        URLProtocol.HTTP ->
            if (allowHttp && url.host in DEBUG_HTTP_HOSTS) Result.success(withScheme)
            else Result.failure(InvalidAddress("Use an https:// address"))
        else -> Result.failure(InvalidAddress("Enter an address like https://squirrel.example.com"))
    }
}
