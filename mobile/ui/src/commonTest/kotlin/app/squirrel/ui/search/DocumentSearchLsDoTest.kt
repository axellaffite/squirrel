package app.squirrel.ui.search

import app.squirrel.ui.files.FilesNavLsDo
import app.squirrel.ui.files.FilesRoute
import app.squirrel.ui.shell.Navigator
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.node
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.milliseconds

/** Document search, and opening it from Files, driven the way an agent does through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class DocumentSearchLsDoTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun hit(id: String) = """{"node":${node(id)},"page_count":3,"snippet":[{"text":"the ","highlight":false},{"text":"tax","highlight":true}]}"""

    private suspend fun vm() = DocumentSearchViewModel(
        testApi { request ->
            if (request.url.encodedPath != "/api/search/documents") return@testApi respondError(HttpStatusCode.NotFound)
            if (request.url.parameters["offset"] == "0") json("""{"items":[${hit("a")},${hit("b")}],"has_more":true}""")
            else json("""{"items":[${hit("c")}],"has_more":false}""")
        },
        handoff = null,
        debounceMs = 20,
    )

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun anAgentTypesWaitsForResultsAndLoadsMore() = runReal {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("document_search", DocumentSearchLsDo(vm()))

        val typed = registry.perform("document_search", intent("""{"type":"type","text":"tax"}"""))
        assertTrue(typed.ok, typed.message)
        val state = typed.screens.screen().state()
        assertEquals("tax", state.string("query"))
        assertEquals("true", state.string("settled"))
        val results = state.getValue("results").jsonArray.map { it.jsonObject }
        assertEquals(listOf("a", "b"), results.map { it.string("id") })
        assertEquals("the **tax**", results.first().string("snippet"))
        assertEquals("PDF · 3 pages · 16 November 2026", results.first().string("meta"))
        assertTrue("more" in typed.screens.screen().actionNames())

        val more = registry.perform("document_search", intent("""{"type":"more"}"""))
        assertTrue(more.ok, more.message)
        assertEquals(listOf("a", "b", "c"), more.screens.screen().state().getValue("results").jsonArray.map { it.jsonObject.string("id") })
        assertFalse("more" in more.screens.screen().actionNames())
    }

    @Test
    fun aShortQueryHasNoSearchAndMoreIsRefusedBeforeResults() = runReal {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("document_search", DocumentSearchLsDo(vm()))

        registry.perform("document_search", intent("""{"type":"type","text":"a"}"""))
        val screen = registry.ls("document_search") ?: error("document_search is listed")
        val unavailable = screen.getValue("unavailable").jsonArray.map { it.jsonPrimitive.content }
        assertTrue(unavailable.containsAll(listOf("search", "more", "retry")), unavailable.toString())
        assertTrue(registry.perform("document_search", intent("""{"type":"search"}""")).message.orEmpty().contains("not available"))
        assertTrue(registry.perform("document_search", intent("""{"type":"more"}""")).message.orEmpty().contains("not available"))
    }

    @Test
    fun filesNavOpensSearchAndGoesBack() = runReal {
        val nav = Navigator<FilesRoute>(FilesRoute.Browse)
        val registry = LsDoRegistry(settle = 10.milliseconds)
        registry.register("files_nav", FilesNavLsDo(nav))

        assertTrue(registry.perform("files_nav", intent("""{"type":"back"}""")).message.orEmpty().contains("not available"))
        val opened = registry.perform("files_nav", intent("""{"type":"search"}"""))
        assertTrue(opened.ok, opened.message)
        assertEquals(FilesRoute.Search, nav.top)
        assertEquals("search", opened.screens.screen().state().string("page"))
        assertEquals(listOf("back"), opened.screens.screen().actionNames())

        val back = registry.perform("files_nav", intent("""{"type":"back"}"""))
        assertTrue(back.ok, back.message)
        assertEquals(FilesRoute.Browse, nav.top)
        assertEquals("browse", back.screens.screen().state().string("page"))
    }

    private fun JsonObject.screen() = getValue("screens").jsonArray.single().jsonObject

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.string(key: String) = getValue(key).jsonPrimitive.content

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.string("name") }
}
