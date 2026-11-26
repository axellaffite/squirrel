package app.squirrel.ui.send

import app.squirrel.api.models.PathSegmentDto
import app.squirrel.ui.files.PickedFile
import app.squirrel.ui.friends.FriendsServer
import app.squirrel.ui.friends.displayName
import app.squirrel.ui.testing.apiError
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.node
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import app.squirrel.ui.testing.text
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** "Send with Squirrel": files handed by another app go into a folder, then are shared with one person. */
class SendModelTest {
    private val calls = mutableListOf<String>()
    private val released = mutableListOf<String>()

    private fun root(id: String, name: String) = node(id, kind = "folder", parent = null).replace("\"name\":\"$id\"", "\"name\":\"$name\"").replace("\"is_root\":false", "\"is_root\":true")

    private fun folder(id: String, name: String, parent: String) = node(id, kind = "folder", parent = parent).replace("\"name\":\"$id\"", "\"name\":\"$name\"")

    private fun share(user: String) =
        """{"id":"s-$user","permission":"read","created_at":"2026-11-20T17:05:00Z","person":{"username":"$user","display_name":"$user"}}"""

    private suspend fun model(scope: CoroutineScope, sentExists: Boolean = false, unknownUser: String? = null, failUpload: String? = null): SendModel {
        val friends = FriendsServer(friends = listOf("anna", "bob"))
        val api = testApi { request ->
            val path = request.url.encodedPath
            val parent = request.url.parameters["parent_id"]
            when {
                path == "/api/friends" -> friends.answer(this, request) ?: respondError(HttpStatusCode.NotFound)
                path == "/api/nodes" && request.method == HttpMethod.Get && parent == null ->
                    json("""{"items":[${root("r-docs", "Documents")},${root("r-files", "Files")},${root("r-photos", "Photos")}],"next_cursor":null}""")
                path == "/api/nodes" && request.method == HttpMethod.Get && parent == "r-files" ->
                    json("""{"items":[${if (sentExists) folder("f-sent", "Sent", "r-files") else folder("f-other", "Taxes", "r-files")}],"next_cursor":null}""")
                path == "/api/folders" || (path == "/api/nodes" && request.method == HttpMethod.Post) -> {
                    calls += "create ${request.text()}"
                    json(folder("f-new", "Sent", "r-files"), HttpStatusCode.Created)
                }
                path == "/api/shares" && request.method == HttpMethod.Post -> {
                    val body = request.text().also { calls += "share $it" }
                    val who = Regex("\"username\":\"([^\"]*)\"").find(body)?.groupValues?.get(1).orEmpty()
                    if (who == unknownUser) apiError(HttpStatusCode.NotFound, "not_found", "No account named $who.") else json(share(who), HttpStatusCode.Created)
                }
                else -> respondError(HttpStatusCode.NotFound)
            }
        }
        val files = listOf("a.pdf", "b.jpg").map { name -> PickedFile(name, open = { null }, release = { released += name }) }
        return SendModel(api, files, scope) { parentId, file, onProgress ->
            if (file.name == failUpload) throw IllegalStateException("disk")
            calls += "upload ${file.name} into $parentId"
            onProgress(1f)
            kotlinx.serialization.json.Json.decodeFromString(app.squirrel.api.models.NodeDto.serializer(), node("n-${file.name.substringBefore('.')}", parent = parentId))
        }
    }

    private fun scoped(block: suspend (CoroutineScope) -> Unit) = runReal {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        try { block(scope) } finally { scope.cancel() }
    }

    @Test
    fun theFilesGoToANewSentFolderInFilesThenAreSharedWithTheFriendPicked() = scoped { scope ->
        val model = model(scope)
        model.load()
        model.state.await { it.friends != null }
        assertEquals(listOf("a.pdf", "b.jpg"), model.state.value.names)
        assertFalse(model.state.value.canSend)

        model.pick("anna")
        assertTrue(model.state.value.canSend)
        model.send()
        val done = model.state.await { it.phase == SendPhase.Sent || it.error != null }
        assertNull(done.error)
        assertEquals("Sent 2 files to ${displayName("anna")}.", done.notice)

        val created = calls.single { it.startsWith("create") }
        assertTrue("\"Sent\"" in created && "r-files" in created, created)
        assertEquals(listOf("upload a.pdf into f-new", "upload b.jpg into f-new"), calls.filter { it.startsWith("upload") })
        val shares = calls.filter { it.startsWith("share") }
        assertEquals(2, shares.size)
        assertTrue(shares.all { "\"anna\"" in it && "\"read\"" in it }, shares.toString())
        assertTrue("\"n-a\"" in shares[0] && "\"n-b\"" in shares[1], shares.toString())
        assertEquals(listOf("a.pdf", "b.jpg"), released)
    }

    @Test
    fun anExistingSentFolderIsReusedAndATypedUsernameWorksToo() = scoped { scope ->
        val model = model(scope, sentExists = true)
        model.load()
        model.type("  cleo ")
        assertEquals("cleo", model.state.value.recipient)
        model.send()
        model.state.await { it.phase == SendPhase.Sent }
        assertFalse(calls.any { it.startsWith("create") })
        assertTrue(calls.filter { it.startsWith("upload") }.all { it.endsWith("f-sent") })
    }

    @Test
    fun typingReplacesTheFriendPickedAndPickingClearsWhatWasTyped() = scoped { scope ->
        val model = model(scope)
        model.pick("anna")
        model.type("zoe")
        assertEquals("zoe", model.state.value.recipient)
        assertNull(model.state.value.picked)
        model.pick("bob")
        assertEquals("", model.state.value.typed)
        assertEquals("bob", model.state.value.recipient)
    }

    @Test
    fun aChosenFolderTakesThePlaceOfSent() = scoped { scope ->
        val model = model(scope)
        model.pick("bob")
        model.choose(listOf(PathSegmentDto("r-docs", "Documents"), PathSegmentDto("f-tax", "Taxes")))
        assertEquals("Documents › Taxes", model.state.value.folderLabel)
        model.send()
        model.state.await { it.phase == SendPhase.Sent }
        assertTrue(calls.filter { it.startsWith("upload") }.all { it.endsWith("f-tax") })
        model.choose(null)
        assertEquals("Files › Sent", model.state.value.folderLabel)
    }

    @Test
    fun aRefusedShareStopsAndSaysTheFileStaysInItsFolder() = scoped { scope ->
        val model = model(scope, unknownUser = "nobody")
        model.type("nobody")
        model.send()
        val failed = model.state.await { it.error != null }
        assertEquals(SendPhase.Editing, failed.phase)
        assertEquals("a.pdf was saved in Files › Sent but not shared: No account named nobody.", failed.error)
        assertEquals(1, calls.count { it.startsWith("upload") })
        assertEquals(listOf("a.pdf", "b.jpg"), released)
    }

    @Test
    fun aFailedUploadNamesTheFile() = scoped { scope ->
        val model = model(scope, failUpload = "b.jpg")
        model.pick("anna")
        model.send()
        val failed = model.state.await { it.error != null }
        assertTrue(failed.error.orEmpty().startsWith("b.jpg could not be sent:"), failed.error)
        assertEquals(1, calls.count { it.startsWith("share") })
    }

    @Test
    fun sendingWithoutARecipientAsksForOne() = scoped { scope ->
        val model = model(scope)
        model.send()
        assertEquals("Choose who to send it to.", model.state.value.error)
        assertTrue(calls.isEmpty())
    }
}
