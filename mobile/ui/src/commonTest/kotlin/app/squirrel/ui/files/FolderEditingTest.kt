package app.squirrel.ui.files

import app.squirrel.api.models.PathSegmentDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.sync.ByteArrayReader
import app.squirrel.shared.sync.FileUpload
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
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withTimeout
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class FolderEditingTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val sent = mutableListOf<Pair<String, String>>()

    private suspend fun api(): SquirrelApi = testApi { request ->
        val path = request.url.encodedPath
        val parent = request.url.parameters["parent_id"]
        if (request.method != HttpMethod.Get) sent += "${request.method.value} $path" to request.text()
        when {
            path == "/api/nodes/d" && request.method == HttpMethod.Get -> json("""{"node":${node("d", "folder")},"path":[],"access":"owner"}""")
            path == "/api/nodes" && parent == "d" -> json("""{"items":[${node("sub", "folder", parent = "d")},${node("a", parent = "d")}],"next_cursor":null}""")
            path == "/api/nodes" && parent == null -> json("""{"items":[${node("d", "folder", parent = null)},${node("other", "folder", parent = null)}],"next_cursor":null}""")
            path == "/api/nodes" -> json("""{"items":[],"next_cursor":null}""")
            path == "/api/folders" && "\"taken\"" in request.text() -> apiError(HttpStatusCode.Conflict, "name_taken", "A file or folder is already called taken.")
            path == "/api/folders" -> json(node("new", "folder", parent = "d"), HttpStatusCode.Created)
            path == "/api/nodes/a" -> json(node("a", parent = "d"))
            else -> respondError(HttpStatusCode.NotFound)
        }
    }

    @Test
    fun aFolderIsCreatedRenamedAndMoved() = runReal {
        val vm = FilesViewModel(api(), null)
        vm.state.await { it.started && !it.loading }
        vm.enter("d")
        val folder = vm.state.await { it.folderId == "d" && it.started && !it.loading }
        assertEquals(true, folder.canAdd)

        assertNull(vm.createFolder("Taxes"))
        assertEquals("A file or folder is already called taken.", vm.createFolder("taken"))
        val file = folder.items.last()
        assertNull(vm.rename(file, "b.pdf"))
        assertNull(vm.move(file, "other"))

        val bodies = sent.map { it.first to it.second }
        assertTrue(bodies[0].second.contains("\"parent_id\":\"d\"") && bodies[0].second.contains("\"name\":\"Taxes\""))
        assertEquals("PATCH /api/nodes/a" to """{"name":"b.pdf"}""", bodies[2])
        assertEquals("PATCH /api/nodes/a" to """{"parent_id":"other"}""", bodies[3])
    }

    @Test
    fun aMoveBrowsesFromTheShareAndNeverOffersTheNodeItself() = runReal {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        try {
            val api = api()
            val sub = app.squirrel.ui.testing.node("sub", "folder", parent = "d")
            val moving = app.squirrel.api.models.NodeDto(
                createdAt = kotlin.time.Instant.parse("2026-11-16T12:00:00Z"), id = "sub", isRoot = false,
                kind = app.squirrel.api.models.NodeKind.folder, modifiedAt = kotlin.time.Instant.parse("2026-11-16T12:00:00Z"),
                name = "sub", propertySize = 0, parentId = "d",
            )
            check(sub.isNotEmpty())
            val model = MoveModel(api, moving, PathSegmentDto("d", "Shared folder"), scope)
            model.load()
            val first = model.state.await { !it.loading }
            assertEquals(emptyList(), first.folders.map { it.id }) // "sub" is the node itself
            assertEquals("d", first.here?.id)
            assertEquals(false, model.canMoveHere) // already there
            assertEquals(false, model.canGoUp)
            assertEquals(false, model.up())

            val top = MoveModel(api, moving, null, scope)
            top.load()
            assertEquals(listOf("d", "other"), top.state.await { !it.loading }.folders.map { it.id })
            assertNull(top.state.value.here)
            top.open(top.state.value.folders[1])
            top.state.await { it.here?.id == "other" && !it.loading }
            assertEquals(true, top.canMoveHere)
            assertEquals(true, top.up())
        } finally {
            scope.cancel()
        }
    }

    @Test
    fun uploadsRunOneByOneAndSayWhatFailed() = runReal {
        val vm = UploadsViewModel(api(), FileUpload(fakeUploadApi()))
        vm.enqueue(listOf(PickedFile("ok.txt", open = { ByteArrayReader("hello".encodeToByteArray()) }), PickedFile("gone.txt", open = { null })), "d")
        assertEquals("d", withTimeout(5_000) { vm.finished.first() })
        val failed = vm.items.await { it.singleOrNull()?.failed == true }.single()
        assertEquals("Uploading gone.txt failed: It could not be read on this phone.", failed.error)
        vm.dismiss(failed.key)
        assertTrue(vm.items.value.isEmpty())
    }

    /** A server that already has every file: the upload links it at once. */
    private suspend fun fakeUploadApi(): SquirrelApi = testApi { request ->
        if (request.url.encodedPath == "/api/uploads") json("""{"deduplicated":true,"node":${node("ok", parent = "d")}}""") else respondError(HttpStatusCode.NotFound)
    }

    @Test
    fun namesAreCheckedLikeTheServerDoes() {
        assertEquals("Enter a name.", nameProblem("  "))
        assertEquals("This name is reserved.", nameProblem(".."))
        assertEquals("Names cannot contain \"/\".", nameProblem("a/b"))
        assertEquals("This name is too long.", nameProblem("é".repeat(200)))
        assertNull(nameProblem(" report.pdf "))
        assertEquals("Album names have at most 100 characters.", albumNameProblem("x".repeat(101)))
        assertEquals("Names cannot contain invisible or control characters.", albumNameProblem("a​b"))
        assertNull(albumNameProblem("Summer / 2024"))
    }
}
