package app.squirrel.ui.sharing

import app.squirrel.api.models.SharePermission
import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import app.squirrel.ui.friends.FriendsServer
import app.squirrel.ui.friends.displayName
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import app.squirrel.ui.testing.text
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.milliseconds

/** The Share sheet's friend picks: one tap shares with the chosen permission; also through LS-DO ("share"). */
class ShareFriendsTest {
    private fun share(user: String, permission: String) =
        """{"id":"s-$user","permission":"$permission","created_at":"2026-11-20T17:05:00Z","person":{"username":"$user","display_name":"${displayName(user)}"}}"""

    private val sent = mutableListOf<String>()

    private suspend fun model(scope: CoroutineScope, friendsUp: Boolean = true): ShareModel {
        val friends = FriendsServer(friends = listOf("anna", "bob", "cleo"))
        return ShareModel(testApi { request ->
            val path = request.url.encodedPath
            when {
                path == "/api/friends" -> if (friendsUp) friends.answer(this, request) ?: respondError(HttpStatusCode.NotFound) else respondError(HttpStatusCode.InternalServerError)
                path == "/api/shares" && request.method == HttpMethod.Get -> json("[${share("anna", "read")}]")
                path == "/api/shares" -> {
                    val body = request.text().also { sent += it }
                    val who = Regex("\"username\":\"([^\"]*)\"").find(body)?.groupValues?.get(1).orEmpty()
                    val permission = if ("\"write\"" in body) "write" else "read"
                    json(share(who, permission), HttpStatusCode.Created)
                }
                path.startsWith("/api/shares/") && request.method == HttpMethod.Delete -> respond("", HttpStatusCode.NoContent)
                else -> respondError(HttpStatusCode.NotFound)
            }
        }, Subject.Node("n1"), scope)
    }

    private fun scoped(block: suspend (CoroutineScope) -> Unit) = runReal {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        try { block(scope) } finally { scope.cancel() }
    }

    @Test
    fun friendsWithoutAccessArePicksThatShareWithTheChosenPermission() = scoped { scope ->
        val model = model(scope)
        model.load()
        val loaded = model.state.await { it.shares != null && it.friends.isNotEmpty() }
        assertEquals(listOf("bob", "cleo"), loaded.picks.map { it.username })

        model.setUsername("typed")
        model.setPermission(SharePermission.write)
        assertNull(model.shareWith("bob"))
        val after = model.state.value
        assertEquals(listOf("anna", "bob"), after.shares?.map { it.person.username })
        assertEquals(SharePermission.write, after.shares?.last()?.permission)
        assertEquals(listOf("cleo"), after.picks.map { it.username })
        assertEquals("typed", after.username, "a pick leaves the typed name alone")
        assertTrue(sent.single().contains("\"node_id\":\"n1\""))
    }

    @Test
    fun theSheetWorksWithoutFriendsWhenTheyFailToLoad() = scoped { scope ->
        val model = model(scope, friendsUp = false)
        model.load()
        val loaded = model.state.await { it.shares != null }
        assertEquals(emptyList(), loaded.picks)
        assertNull(loaded.error)
        model.setUsername("dan")
        model.share()
        assertEquals(listOf("anna", "dan"), model.state.await { it.shares?.size == 2 }.shares?.map { it.person.username })
    }

    @Test
    fun anAgentSharesWithAFriendAndClosesTheSheet() = scoped { scope ->
        val model = model(scope)
        var closed = false
        val registry = LsDoRegistry(settle = 50.milliseconds)
        val screen = ShareLsDo(model, "Taxes", onClose = { closed = true })
        registry.register("share", screen)
        model.load()
        screen.state.await { it.shares != null && it.picks.isNotEmpty() }

        val listed = registry.ls("share") ?: error("share is listed")
        assertEquals("Taxes", listed.state().string("name"))
        assertEquals(listOf("bob", "cleo"), listed.state().rows("picks").map { it.string("username") })

        val permission = registry.perform("share", intent("""{"type":"set_permission","canEdit":true}"""))
        assertTrue(permission.ok, permission.message)
        val picked = registry.perform("share", intent("""{"type":"share_with","username":"cleo"}"""))
        assertTrue(picked.ok, picked.message)
        val state = picked.screens.screen("share").state()
        assertEquals(listOf("anna", "cleo"), state.rows("shares").map { it.string("username") })
        assertEquals("true", state.rows("shares").last().string("canEdit"))
        assertEquals(listOf("bob"), state.rows("picks").map { it.string("username") })

        assertEquals("\"anna\" already has access.", registry.perform("share", intent("""{"type":"share_with","username":"anna"}""")).message)
        assertEquals("\"zed\" is not a friend.", registry.perform("share", intent("""{"type":"share_with","username":"zed"}""")).message)

        val typed = registry.perform("share", intent("""{"type":"share","username":"dan"}""")).screens.screen("share").state()
        assertEquals(listOf("anna", "cleo", "dan"), typed.rows("shares").map { it.string("username") })

        val changed = registry.perform("share", intent("""{"type":"change","username":"anna","canEdit":true}""")).screens.screen("share").state()
        assertEquals("true", changed.rows("shares").first().string("canEdit"))

        val stopped = registry.perform("share", intent("""{"type":"stop","username":"anna"}""")).screens.screen("share").state()
        assertFalse("anna" in stopped.rows("shares").map { it.string("username") })

        assertTrue(registry.perform("share", intent("""{"type":"close"}""")).ok)
        assertTrue(closed)
    }

    private fun intent(text: String) = Json.parseToJsonElement(text)

    private fun JsonObject.screen(id: String) = getValue("screens").jsonArray.map { it.jsonObject }.single { it.string("id") == id }

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.rows(key: String) = getValue(key).jsonArray.map { it.jsonObject }

    private fun JsonObject.string(key: String) = getValue(key).jsonPrimitive.content
}
