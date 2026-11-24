package app.squirrel.ui.search

import app.squirrel.lsdo.runtime.LsDoRegistry
import app.squirrel.ui.photos.PhotosOverlay
import app.squirrel.ui.testing.apiError
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
import app.squirrel.ui.testing.testApi
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode
import io.ktor.http.Parameters
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
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

@OptIn(ExperimentalCoroutinesApi::class)
class PhotoSearchLsDoTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() {
        scope.cancel()
        Dispatchers.resetMain()
    }

    private val asked = mutableListOf<Parameters>()

    private suspend fun screen(available: Boolean = true): PhotoSearchLsDo {
        val vm = PhotoSearchViewModel(testApi { request ->
            when (request.url.encodedPath) {
                "/api/search/warmup" -> json("", HttpStatusCode.NoContent)
                "/api/people" -> json("""[{"id":"anna","name":"Anna","hidden":false,"face_count":3}]""")
                "/api/search/photos" -> {
                    asked += request.url.parameters
                    if (available) json("""{"items":[${summary("p1")},${summary("p2")}],"has_more":false,"next_cursor":null}""")
                    else apiError(HttpStatusCode.ServiceUnavailable, "ml_unavailable")
                }
                else -> respondError(HttpStatusCode.NotFound)
            }
        }, debounceMs = 1, busyRetryMs = 1)
        lateinit var lsdo: PhotoSearchLsDo
        lsdo = PhotoSearchLsDo(vm, scope, onOpen = { lsdo.overlay.shown(PhotosOverlay(viewing = it)) }, onClose = { lsdo.overlay.shown(PhotosOverlay()) })
        return lsdo
    }

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun anAgentSearchesFiltersAndOpensAPhoto() = runReal {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("photo_search", screen())

        val found = registry.perform("photo_search", intent("""{"type":"search","text":"dog on the beach"}"""))
        assertTrue(found.ok, found.message)
        val state = found.screens.screen().state()
        assertEquals("dog on the beach", state["query"]?.jsonPrimitive?.content)
        assertEquals(listOf("p1", "p2"), state.getValue("photos").jsonArray.map { it.jsonObject.getValue("id").jsonPrimitive.content })
        assertFalse("more" in found.screens.screen().actionNames())

        assertTrue(registry.perform("photo_search", intent("""{"type":"filter","filter":{"person":"anna"}}""")).ok)
        assertEquals("anna", asked.last()["person"])
        assertTrue(registry.perform("photo_search", intent("""{"type":"order","order":"relevance"}""")).ok)
        assertEquals("relevance", asked.last()["order"])

        assertEquals("No photo \"zz\" in the results.", registry.perform("photo_search", intent("""{"type":"open","id":"zz"}""")).message)
        val open = registry.perform("photo_search", intent("""{"type":"open","id":"p1"}"""))
        assertEquals("p1", open.screens.screen().state()["viewing"]?.jsonPrimitive?.content)
        assertTrue("close_photo" in open.screens.screen().actionNames())
    }

    @Test
    fun aServerWithoutSearchOffersNeitherOrderNorRetry() = runReal {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("photo_search", screen(available = false))

        val done = registry.perform("photo_search", intent("""{"type":"search","text":"cat"}"""))
        val screen = done.screens.screen()
        assertEquals(true, screen.state()["unavailable"]?.jsonPrimitive?.content?.toBoolean())
        assertFalse(screen.actionNames().any { it == "order" || it == "retry" })
        assertTrue(registry.perform("photo_search", intent("""{"type":"order","order":"relevance"}""")).message.orEmpty().contains("not available"))
    }

    private fun JsonObject.screen() = getValue("screens").jsonArray.single().jsonObject

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content }
}
