package app.squirrel.ui.search

import app.squirrel.api.models.SnippetPart
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.HandoffResult
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.node
import app.squirrel.ui.testing.runReal
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
import kotlin.test.assertNull

@OptIn(ExperimentalCoroutinesApi::class)
class DocumentSearchViewModelTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun hit(id: String) = """{"node":${node(id)},"page_count":3,"snippet":[{"text":"the ","highlight":false},{"text":"tax","highlight":true}]}"""

    private val offsets = mutableListOf<String?>()

    private suspend fun vm(handoff: FileHandoff? = null, status: HttpStatusCode? = null) = DocumentSearchViewModel(
        testApi { request ->
            if (request.url.encodedPath != "/api/search/documents") return@testApi respondError(HttpStatusCode.NotFound)
            offsets += request.url.parameters["offset"]
            when {
                status != null -> respondError(status)
                request.url.parameters["offset"] == "0" -> json("""{"items":[${hit("a")},${hit("b")}],"has_more":true}""")
                else -> json("""{"items":[${hit("b")},${hit("c")}],"has_more":false}""")
            }
        },
        handoff,
        debounceMs = 1,
    )

    @Test
    fun resultsArriveAndLoadMoreAppendsWithoutDuplicates() = runReal {
        val vm = vm()
        vm.type("tax")
        val first = vm.state.await { it.settled }
        assertEquals(listOf("a", "b"), first.items.map { it.node.id })
        assertEquals("PDF · 3 pages · 16 November 2026", documentMeta(first.items.first()))
        vm.more()
        val all = vm.state.await { it.items.size == 3 }
        assertEquals(listOf("a", "b", "c"), all.items.map { it.node.id })
        assertEquals(false, all.hasMore)
        assertEquals(listOf<String?>("0", "2"), offsets.toList())
    }

    @Test
    fun oneLetterIsNotSentAndShortWordsGetAHint() = runReal {
        val vm = vm()
        vm.type("a")
        assertEquals(emptyList<String?>(), offsets)
        vm.type("an invoice")
        assertEquals("Two-letter words only match whole words.", vm.state.value.hint)
    }

    @Test
    fun aSlowSearchIsWordedCalmly() = runReal {
        val vm = vm(status = HttpStatusCode.ServiceUnavailable)
        vm.type("tax")
        assertEquals("The search took too long. Try more precise words.", vm.state.await { it.error != null && !it.loading }.error)
    }

    @Test
    fun aResultOpensWithAnotherApp() = runReal {
        val opened = mutableListOf<String>()
        val handoff = object : FileHandoff {
            override suspend fun open(api: SquirrelApi, id: String, name: String, mime: String?, share: Boolean, onProgress: (Float) -> Unit): HandoffResult {
                opened += id
                return HandoffResult.NoApp
            }
            override fun clear() = Unit
        }
        val vm = vm(handoff)
        vm.type("tax")
        val hit = vm.state.await { it.settled }.items.first()
        vm.opener.open(hit.node)
        assertEquals("No app on this phone opens a.pdf.", vm.opener.state.await { it.message != null }.message)
        assertEquals(listOf("a"), opened)
        assertNull(vm.opener.state.value.confirm)
    }

    @Test
    fun snippetsMergeAndCut() {
        val parts = listOf(SnippetPart(highlight = false, text = "ab"), SnippetPart(highlight = false, text = "cd"), SnippetPart(highlight = true, text = "efgh"))
        assertEquals(listOf(SnippetPart(highlight = false, text = "abcd"), SnippetPart(highlight = true, text = "e…")), snippetParts(parts, max = 5))
    }
}
