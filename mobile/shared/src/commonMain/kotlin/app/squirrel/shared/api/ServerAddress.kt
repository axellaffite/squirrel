package app.squirrel.shared.api

import io.ktor.http.URLProtocol
import io.ktor.http.Url

const val INVALID_ADDRESS = "Enter an address like https://squirrel.example.com"
const val HTTPS_REQUIRED = "Use https:// for an address outside your home network or VPN"

/** A request to a public host over plain HTTP: refused before anything is sent. */
class CleartextRefused(host: String) : Exception("Refusing to send to $host without HTTPS")

private val PRIVATE_SUFFIXES = listOf(".local", ".lan", ".home.arpa", ".ts.net")
private val HOSTNAME = Regex("^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)*$")
private val IPV6 = Regex("^[0-9a-f:.]+$")

/**
 * Hosts the app may reach over plain HTTP: RFC 1918, Tailscale (100.64.0.0/10, `*.ts.net`),
 * mDNS and home names (`.local`, `.lan`, `.home.arpa`), IPv6 ULA and link-local, loopback.
 * Decided on the name as typed: DNS is never consulted.
 */
fun isPrivateHost(rawHost: String): Boolean {
    val host = rawHost.lowercase().removePrefix("[").removeSuffix("]").trimEnd('.')
    if (host == "localhost") return true
    ipv4(host)?.let { (a, b) ->
        return a == 10 || a == 127 || (a == 172 && b in 16..31) || (a == 192 && b == 168) || (a == 100 && b in 64..127)
    }
    if (':' in host) return ipv6Private(host)
    return PRIVATE_SUFFIXES.any { host.endsWith(it) && host.length > it.length }
}

private fun ipv4(host: String): Pair<Int, Int>? {
    val parts = host.split('.')
    if (parts.size != 4) return null
    val octets = parts.map { part -> part.toIntOrNull()?.takeIf { it in 0..255 && part.length <= 3 } ?: return null }
    return octets[0] to octets[1]
}

private fun ipv6Private(host: String): Boolean {
    if (!IPV6.matches(host)) return false
    if (host == "::1") return true
    val first = if (host.startsWith("::")) 0 else host.substringBefore(':').toIntOrNull(16) ?: return false
    return first and 0xfe00 == 0xfc00 || first and 0xffc0 == 0xfe80
}

private fun validHost(host: String): Boolean {
    val bare = host.lowercase().removePrefix("[").removeSuffix("]").trimEnd('.')
    return bare.isNotEmpty() && (HOSTNAME.matches(bare) || (':' in bare && IPV6.matches(bare)))
}

/** May a request go to [url]? HTTPS anywhere; HTTP only to [isPrivateHost]. */
fun cleartextAllowed(url: Url): Boolean = url.protocol != URLProtocol.HTTP || isPrivateHost(url.host)

/**
 * Turns what the user typed into `scheme://host[:port][/path]`: adds `https://`, drops a trailing
 * slash, accepts `http://` only for [isPrivateHost] addresses. Debug and release builds alike.
 */
fun normalizeServerUrl(input: String): Result<String> {
    val trimmed = input.trim().trimEnd('/')
    val bareScheme = trimmed.endsWith(":") && trimmed.dropLast(1).all { it.isLetter() }
    if (trimmed.isEmpty() || bareScheme || trimmed.any { it.isWhitespace() }) return Result.failure(IllegalArgumentException(INVALID_ADDRESS))
    val withScheme = if ("://" in trimmed) trimmed else "https://$trimmed"
    val url = runCatching { Url(withScheme) }.getOrNull()
    if (url == null || !validHost(url.host) || url.parameters.names().isNotEmpty() || url.fragment.isNotEmpty() ||
        url.protocol !in setOf(URLProtocol.HTTP, URLProtocol.HTTPS) || !withScheme.substringBefore("://").let { it == "http" || it == "https" }
    ) {
        return Result.failure(IllegalArgumentException(INVALID_ADDRESS))
    }
    if (!cleartextAllowed(url)) return Result.failure(IllegalArgumentException(HTTPS_REQUIRED))
    return Result.success(withScheme)
}

/** True when [serverUrl] is plain HTTP (the sign-in screen says so). */
fun isCleartext(serverUrl: String): Boolean = serverUrl.startsWith("http://")
