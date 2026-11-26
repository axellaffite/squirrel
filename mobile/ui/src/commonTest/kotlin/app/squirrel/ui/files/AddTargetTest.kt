package app.squirrel.ui.files

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.node
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import app.squirrel.ui.testing.text
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** The bottom bar's add menu: into the folder open in Files, otherwise the top-level "Files" folder. */
@OptIn(ExperimentalCoroutinesApi::class)
class AddTargetTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val sent = mutableListOf<String>()

    /** A top-level folder, as the server creates them for a new user (Photos, Documents, Files). */
    private fun root(id: String, name: String) =
        node(id, "folder", parent = null).replace("\"name\":\"$id\"", "\"name\":\"$name\"").replace("\"is_root\":false", "\"is_root\":true")

    private suspend fun api(topLevel: Boolean = true): SquirrelApi = testApi { request ->
        val path = request.url.encodedPath
        val parent = request.url.parameters["parent_id"]
        if (request.method != HttpMethod.Get) sent += request.text()
        when {
            path == "/api/nodes/d" -> json("""{"node":${node("d", "folder", parent = "files")},"path":[],"access":"owner"}""")
            path == "/api/nodes" && parent == null && !topLevel -> respondError(HttpStatusCode.ServiceUnavailable)
            path == "/api/nodes" && parent == null ->
                json("""{"items":[${root("photos", "Photos")},${root("docs", "Documents")},${root("files", "Files")}],"next_cursor":null}""")
            path == "/api/nodes" -> json("""{"items":[],"next_cursor":null}""")
            path == "/api/folders" -> json(node("new", "folder", parent = "files"), HttpStatusCode.Created)
            else -> respondError(HttpStatusCode.NotFound)
        }
    }

    @Test
    fun atTheTopLevelItIsTheFilesFolder() = runReal {
        val vm = FilesViewModel(api(), null)
        vm.state.await { it.started && !it.loading }
        assertEquals("files", vm.addTarget())
    }

    @Test
    fun insideAFolderItIsThatFolder() = runReal {
        val vm = FilesViewModel(api(), null)
        vm.enter("d")
        vm.state.await { it.folderId == "d" && it.started && !it.loading }
        assertEquals("d", vm.addTarget())
    }

    @Test
    fun theTopLevelIsAskedForWhenItIsNotListedYet() = runReal {
        val vm = FilesViewModel(api(), null)
        vm.enter("d")
        vm.state.await { it.folderId == "d" && it.started && !it.loading }
        // A folder the caller may not add to (nothing loaded about it yet): the Files folder instead.
        vm.enter("elsewhere")
        assertEquals("files", vm.addTarget())
    }

    @Test
    fun anUnreachableServerHasNoTarget() = runReal {
        val vm = FilesViewModel(api(topLevel = false), null)
        vm.state.await { !it.loading }
        assertNull(vm.addTarget())
    }

    @Test
    fun aFolderIsCreatedInTheTargetAndShown() = runReal {
        val vm = FilesViewModel(api(), null)
        vm.state.await { it.started && !it.loading }
        assertNull(vm.createFolderIn("files", "Taxes"))
        assertTrue(sent.single().contains("\"parent_id\":\"files\"") && sent.single().contains("\"name\":\"Taxes\""))
        vm.show("files")
        assertEquals("files", vm.state.value.folderId)
    }
}
