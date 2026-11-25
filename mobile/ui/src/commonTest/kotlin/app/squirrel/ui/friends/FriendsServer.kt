package app.squirrel.ui.friends

import app.squirrel.ui.testing.apiError
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.text
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.client.request.HttpRequestData
import io.ktor.client.request.HttpResponseData
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode

fun displayName(user: String) = user.replaceFirstChar { it.uppercase() }

fun friendJson(user: String, name: String = displayName(user)) =
    """{"username":"$user","display_name":"$name","added_at":"2026-11-20T17:05:00Z"}"""

fun memberJson(user: String) = """{"username":"$user","display_name":"${displayName(user)}"}"""

/**
 * The friends routes as the server answers them, with its state: by default anna is a friend, bob
 * and cleo are suggestions. Adding takes a suggestion out; removing someone puts them back first
 * in the suggestions (they share with the caller). "nobody" is refused (422), and removing someone
 * not on the list is a 404. Calls are recorded in [added] and [removed].
 */
class FriendsServer(
    friends: List<String> = listOf("anna"),
    suggestions: List<String> = listOf("bob", "cleo"),
) {
    val added = mutableListOf<String>()
    val removed = mutableListOf<String>()
    private var friends = friends.sorted()
    private var suggestions = suggestions

    /** Someone leaves the list on the server's side (another device, a deleted account). */
    fun forget(user: String) {
        friends = friends - user
    }

    /** Answers a friends route, or null when [request] is for another one. */
    fun answer(scope: MockRequestHandleScope, request: HttpRequestData): HttpResponseData? = with(scope) {
        val path = request.url.encodedPath
        when {
            path == "/api/friends" && request.method == HttpMethod.Get -> json(
                """{"friends":[${friends.joinToString(",") { friendJson(it) }}],"suggestions":[${suggestions.joinToString(",") { memberJson(it) }}]}""",
            )
            path == "/api/friends" && request.method == HttpMethod.Post -> {
                val who = Regex("\"username\":\"([^\"]*)\"").find(request.text())?.groupValues?.get(1).orEmpty()
                added += who
                if (who == "nobody") return@with apiError(HttpStatusCode.UnprocessableEntity, "validation", "No account is called nobody.")
                friends = (friends + who).distinct().sorted()
                suggestions = suggestions - who
                json(friendJson(who), HttpStatusCode.Created)
            }
            path.startsWith("/api/friends/") && request.method == HttpMethod.Delete -> {
                val who = path.substringAfterLast('/')
                removed += who
                if (who !in friends) return@with apiError(HttpStatusCode.NotFound, "not_found", "Not a friend.")
                friends = friends - who
                suggestions = listOf(who) + suggestions
                respond("", HttpStatusCode.NoContent)
            }
            else -> null
        }
    }

    fun handler(): suspend MockRequestHandleScope.(HttpRequestData) -> HttpResponseData = { request ->
        answer(this, request) ?: respondError(HttpStatusCode.NotFound)
    }
}
