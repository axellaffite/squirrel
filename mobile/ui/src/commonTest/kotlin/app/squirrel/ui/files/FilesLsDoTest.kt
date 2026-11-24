package app.squirrel.ui.files

import app.squirrel.lsdo.runtime.LsDoRegistry
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.Json
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.milliseconds

/** The Files screen driven the way an agent does: ls, then do, through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class FilesLsDoTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun node(id: String, kind: String = "file", parent: String = "root") =
        """{"id":"$id","parent_id":"$parent","name":"$id","kind":"$kind","size":1000,"mime":null,"is_root":false,""" +
            """"created_at":"2026-11-16T12:00:00Z","modified_at":"2026-11-16T12:00:00Z","thumbnail":null}"""

    private val renamed = mutableListOf<String>()

    private suspend fun api(): SquirrelApi {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
        val json = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())
        val engine = MockEngine { request ->
            val path = request.url.encodedPath
            val parent = request.url.parameters["parent_id"] ?: request.url.parameters["parent"]
            when {
                path == "/api/nodes" && parent == null -> respond("""{"items":[${node("docs", "folder")}],"next_cursor":null}""", headers = json)
                path == "/api/nodes" && parent == "docs" -> respond("""{"items":[${node("tax.pdf", parent = "docs")}],"next_cursor":null}""", headers = json)
                path == "/api/nodes/docs" -> respond("""{"node":${node("docs", "folder")},"path":[]}""", headers = json)
                path == "/api/nodes/tax.pdf" && request.method == HttpMethod.Patch -> {
                    renamed += "tax.pdf"
                    respond(node("tax.pdf", parent = "docs"), headers = json)
                }
                else -> respondError(HttpStatusCode.NotFound)
            }
        }
        return SquirrelApi("https://home.example", engine, session)
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun anAgentListsTheRootThenEntersAFolderAndRenamesARow() = run {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("files", FilesViewModel(api(), handoff = null))

        val entered = registry.perform("files", intent("""{"type":"enter","id":"docs"}"""))
        assertTrue(entered.ok, entered.message)
        val screen = entered.screens.screen()
        assertEquals("docs", screen.state()["folderId"]?.jsonPrimitive?.content)
        assertEquals(listOf("tax.pdf"), screen.state().getValue("items").jsonArray.map { it.jsonObject.getValue("id").jsonPrimitive.content })
        assertTrue("up" in screen.actionNames() && "rename" in screen.actionNames())

        val done = registry.perform("files", intent("""{"type":"rename","id":"tax.pdf","name":"taxes.pdf"}"""))
        assertTrue(done.ok, done.message)
        assertEquals(listOf("tax.pdf"), renamed)
    }

    @Test
    fun theRootHasNoUpAndAMissingRowIsRefused() = run {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("files", FilesViewModel(api(), handoff = null))
        registry.perform("files", intent("""{"type":"refresh"}"""))

        val root = registry.ls("files") ?: error("files is listed")
        assertFalse("up" in root.actionNames())
        assertFalse("create_folder" in root.actionNames())
        assertTrue(registry.perform("files", intent("""{"type":"up"}""")).message.orEmpty().contains("not available"))
        assertEquals("No row \"nope\" in the folder shown.", registry.perform("files", intent("""{"type":"open","id":"nope"}""")).message)
    }

    @Test
    fun enterWithoutAnIdStaysInsideASharedFolder() = run {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("shared", FilesViewModel(api(), handoff = null, root = "docs"))

        val back = registry.perform("shared", intent("""{"type":"enter"}"""))

        assertTrue(back.ok, back.message)
        assertEquals("docs", back.screens.screen().state()["folderId"]?.jsonPrimitive?.content)
        assertFalse("up" in back.screens.screen().actionNames())
    }

    private fun JsonObject.screen() = getValue("screens").jsonArray.single().jsonObject

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content }
}
