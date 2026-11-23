package app.squirrel.ui.files

import app.squirrel.api.models.AccessLevel
import app.squirrel.ui.photos.AlbumViewModel
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.node
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
import app.squirrel.ui.testing.testApi
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalCoroutinesApi::class)
class SharedFolderTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val anna = """{"username":"anna","display_name":"Anna"}"""

    private suspend fun api(access: String) = testApi { request ->
        val path = request.url.encodedPath
        val parent = request.url.parameters["parent_id"]
        when {
            path == "/api/nodes/s" -> json("""{"node":${node("s", "folder", parent = null)},"path":[],"access":"$access","shared_by":$anna}""")
            path == "/api/nodes/c" -> json("""{"node":${node("c", "folder", parent = "s")},"path":[{"id":"s","name":"s"}],"access":"$access","shared_by":$anna}""")
            path == "/api/nodes" && parent == "s" -> json("""{"items":[${node("c", "folder", parent = "s")}],"next_cursor":null}""")
            path == "/api/nodes" && parent == "c" -> json("""{"items":[],"next_cursor":null}""")
            else -> respondError(HttpStatusCode.NotFound)
        }
    }

    @Test
    fun aSharedFolderStartsAtTheShareAndNeverGoesAbove() = runReal {
        val vm = FilesViewModel(api("write"), null, root = "s")
        val top = vm.state.await { it.started && !it.loading }
        assertEquals("s", top.folderId)
        assertEquals(AccessLevel.write, top.access)
        assertEquals("Anna", top.sharedBy?.displayName)
        assertEquals(true, top.canAdd)
        assertEquals(false, top.can.owner)
        assertEquals(true, vm.atRoot)

        vm.enter("c")
        val child = vm.state.await { it.folderId == "c" && it.started && !it.loading }
        assertEquals(listOf("s", "c"), child.path.map { it.id })
        assertEquals(true, vm.up())
        assertEquals("s", vm.state.await { it.folderId == "s" && !it.loading }.folderId)
        assertEquals(false, vm.up())
    }

    @Test
    fun aReaderMayNotAdd() = runReal {
        val vm = FilesViewModel(api("read"), null, root = "s")
        val top = vm.state.await { it.started && !it.loading }
        assertEquals(false, top.canAdd)
        assertEquals(false, top.can.edit)
    }

    @Test
    fun anAlbumSaysWhatTheCallerMayDo() = runReal {
        val vm = AlbumViewModel(testApi { request ->
            when (request.url.encodedPath) {
                "/api/albums/al" -> json("""{"id":"al","name":"Holidays","item_count":1,"updated_at":"2026-11-20T17:05:00Z","access":"write","shared_by":$anna}""")
                "/api/albums/al/items" -> json("""{"items":[${summary("p")}],"next_cursor":null}""")
                else -> respondError(HttpStatusCode.NotFound)
            }
        }, "al")
        val state = vm.state.await { it.album != null }
        assertEquals(false, state.can.owner)
        assertEquals(true, state.can.edit)
    }
}
