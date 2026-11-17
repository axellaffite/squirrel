package app.squirrel.ui.files

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

@OptIn(ExperimentalCoroutinesApi::class)
class FilesViewModelTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun node(id: String, kind: String = "file", size: Long = 1000, parent: String = "root") =
        """{"id":"$id","parent_id":"$parent","name":"$id${if (kind == "file") ".pdf" else ""}","kind":"$kind","size":$size,""" +
            """"mime":${if (kind == "file") "\"application/pdf\"" else "null"},"is_root":false,""" +
            """"created_at":"2026-11-16T12:00:00Z","modified_at":"2026-11-16T12:00:00Z","thumbnail":null}"""

    private suspend fun api(): SquirrelApi {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
        val json = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())
        val engine = MockEngine { request ->
            val path = request.url.encodedPath
            val parent = request.url.parameters["parent_id"] ?: request.url.parameters["parent"]
            when {
                path == "/api/nodes" && parent == null && request.url.parameters["cursor"] == null ->
                    respond("""{"items":[${node("docs", "folder")},${node("a")}],"next_cursor":"c2"}""", headers = json)
                path == "/api/nodes" && parent == null ->
                    respond("""{"items":[${node("b")},${node("big", size = 1_400_000_000)}],"next_cursor":null}""", headers = json)
                path == "/api/nodes" && parent == "docs" ->
                    respond("""{"items":[${node("tax", "folder", parent = "docs")}],"next_cursor":null}""", headers = json)
                path == "/api/nodes/docs" ->
                    respond("""{"node":${node("docs", "folder")},"path":[]}""", headers = json)
                else -> respondError(HttpStatusCode.NotFound)
            }
        }
        return SquirrelApi("https://home.example", engine, session)
    }

    private class FakeHandoff : FileHandoff {
        val opened = mutableListOf<String>()
        val gate = CompletableDeferred<Unit>()
        override suspend fun open(api: SquirrelApi, id: String, name: String, mime: String?, share: Boolean, onProgress: (Float) -> Unit): HandoffResult {
            onProgress(0.5f)
            gate.await()
            opened += id
            return HandoffResult.Opened
        }
        override fun clear() = Unit
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private suspend fun FilesViewModel.settled() = withTimeout(5_000) { state.first { !it.loading } }

    @Test
    fun theRootPagesWithItsCursor() = run {
        val vm = FilesViewModel(api(), null)
        assertEquals(listOf("docs", "a"), vm.settled().items.map { it.id })
        assertEquals("Files", vm.state.value.title)
        vm.more()
        val all = vm.settled()
        assertEquals(listOf("docs", "a", "b", "big"), all.items.map { it.id })
        assertEquals(true, all.complete)
    }

    @Test
    fun foldersShowTheirBreadcrumbsAndGoBackUp() = run {
        val vm = FilesViewModel(api(), null)
        val docs = vm.settled().items.first()
        vm.open(docs)
        val inside = vm.settled()
        assertEquals(listOf("docs"), inside.path.map { it.name })
        assertEquals("docs", inside.title)
        assertEquals(listOf("tax"), inside.items.map { it.id })

        assertEquals(true, vm.up())
        assertEquals(null, vm.settled().folderId)
        assertEquals(false, vm.up())
    }

    @Test
    fun aFileDownloadsWithProgressThenOpens() = run {
        val handoff = FakeHandoff()
        val vm = FilesViewModel(api(), handoff)
        val file = vm.settled().items[1]
        vm.open(file)
        assertEquals(0.5f, withTimeout(5_000) { vm.state.first { it.downloads[file.id] == 0.5f } }.downloads[file.id])
        handoff.gate.complete(Unit)
        withTimeout(5_000) { vm.state.first { file.id !in it.downloads } }
        assertEquals(listOf("a"), handoff.opened)
    }

    @Test
    fun aLargeFileAsksFirst() = run {
        val handoff = FakeHandoff().apply { gate.complete(Unit) }
        val vm = FilesViewModel(api(), handoff)
        vm.settled()
        vm.more()
        val big = vm.settled().items.last()
        vm.open(big)
        assertEquals("Download 1.4 GB?", vm.state.value.confirmText)
        assertEquals(emptyList(), handoff.opened)
        vm.open(big, confirmed = true)
        withTimeout(5_000) { vm.state.first { it.downloads.isEmpty() && handoff.opened.isNotEmpty() } }
        assertNull(vm.state.value.confirm)
        assertEquals(listOf("big"), handoff.opened)
    }
}
