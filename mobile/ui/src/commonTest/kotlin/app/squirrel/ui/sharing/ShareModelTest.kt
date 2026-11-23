package app.squirrel.ui.sharing

import app.squirrel.api.models.SharePermission
import app.squirrel.ui.testing.apiError
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
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ShareModelTest {
    private fun share(id: String, user: String, permission: String) =
        """{"id":"$id","permission":"$permission","created_at":"2026-11-20T17:05:00Z","person":{"username":"$user","display_name":"${user.replaceFirstChar { it.uppercase() }}"}}"""

    private val sent = mutableListOf<String>()

    private suspend fun model(scope: CoroutineScope, subject: Subject = Subject.Album("al")) = ShareModel(testApi { request ->
        val path = request.url.encodedPath
        when {
            path == "/api/shares" && request.method == HttpMethod.Get -> {
                assertEquals("al", request.url.parameters["album_id"])
                json("[${share("s1", "anna", "read")}]")
            }
            path == "/api/shares" -> {
                val body = request.text()
                sent += body
                when {
                    "\"nobody\"" in body -> apiError(HttpStatusCode.UnprocessableEntity, "no_such_account", "No account is called nobody.")
                    "\"write\"" in body && "\"anna\"" in body -> json(share("s1", "anna", "write"), HttpStatusCode.Created)
                    else -> json(share("s2", "bob", "write"), HttpStatusCode.Created)
                }
            }
            path == "/api/shares/s1" && request.method == HttpMethod.Delete -> respond("", HttpStatusCode.NoContent)
            else -> respondError(HttpStatusCode.NotFound)
        }
    }, subject, scope)

    private fun scoped(block: suspend (CoroutineScope) -> Unit) = runReal {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        try { block(scope) } finally { scope.cancel() }
    }

    @Test
    fun sharingAddsSomeoneAndEmptiesTheField() = scoped { scope ->
        val model = model(scope)
        model.load()
        assertEquals(listOf("anna"), model.state.await { it.shares != null }.shares?.map { it.person.username })
        model.setUsername("  bob ")
        model.setPermission(SharePermission.write)
        model.share()
        val after = model.state.await { it.shares?.size == 2 }
        assertEquals("", after.username)
        assertTrue(sent.single().contains("\"username\":\"bob\""))
        assertTrue(sent.single().contains("\"album_id\":\"al\""))
        assertTrue(!sent.single().contains("node_id"))
    }

    @Test
    fun aPermissionChangesAndSharingStops() = scoped { scope ->
        val model = model(scope)
        model.load()
        val anna = model.state.await { it.shares != null }.shares.orEmpty().single()
        model.change(anna, SharePermission.write)
        assertEquals(SharePermission.write, model.state.await { it.shares?.single()?.permission == SharePermission.write }.shares?.single()?.permission)
        model.stop(anna)
        assertTrue(model.state.await { it.shares?.isEmpty() == true }.shares.orEmpty().isEmpty())
    }

    @Test
    fun theServersRefusalIsShownAndAnEmptyNameIsCaught() = scoped { scope ->
        val model = model(scope)
        model.share()
        assertEquals("Enter a username.", model.state.value.error)
        model.setUsername("nobody")
        model.share()
        assertEquals("No account is called nobody.", model.state.await { it.error != "Enter a username." && it.error != null }.error)
        assertEquals("nobody", model.state.value.username)
    }
}
