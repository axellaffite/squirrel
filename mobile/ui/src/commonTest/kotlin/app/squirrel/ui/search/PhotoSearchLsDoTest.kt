package app.squirrel.ui.search

import app.squirrel.ui.photos.PhotosOverlay
import app.squirrel.ui.testing.apiError
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
import app.squirrel.ui.testing.testApi
import app.squirrel.ui.testing.text
import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpMethod
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
    private val sent = mutableListOf<String>()
    private val added = mutableListOf<String>()

    private suspend fun screen(available: Boolean = true): PhotoSearchLsDo {
        val api = testApi { request ->
            when (request.url.encodedPath) {
                "/api/search/warmup" -> json("", HttpStatusCode.NoContent)
                "/api/people" -> json("""[{"id":"anna","name":"Anna","hidden":false,"face_count":3}]""")
                "/api/search/photos" -> {
                    asked += request.url.parameters
                    if (available) json("""{"items":[${summary("p1")},${summary("p2")}],"has_more":false,"next_cursor":null}""")
                    else apiError(HttpStatusCode.ServiceUnavailable, "ml_unavailable")
                }
                "/api/albums" -> if (request.method == HttpMethod.Post) {
                    sent += "create ${request.text()}"
                    json(ALBUM.replace("\"al\"", "\"new\"").replace("Holidays", "Dogs"))
                } else {
                    json("[$ALBUM]")
                }
                "/api/shared-with-me" -> json("[]")
                "/api/albums/al/items", "/api/albums/new/items" -> {
                    sent += "add ${request.url.encodedPath} ${request.text()}"
                    json("""{"changed":2}""")
                }
                else -> respondError(HttpStatusCode.NotFound)
            }
        }
        val vm = PhotoSearchViewModel(api, debounceMs = 1, busyRetryMs = 1)
        lateinit var lsdo: PhotoSearchLsDo
        lsdo = PhotoSearchLsDo(
            vm,
            api,
            scope,
            onOpen = { lsdo.overlay.shown(PhotosOverlay(viewing = it)) },
            onClose = { lsdo.overlay.shown(PhotosOverlay()) },
            onAdded = { added += it },
        )
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
    fun anAgentAddsFoundPhotosToAnAlbumOrANewOne() = runReal {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("photo_search", screen())
        assertTrue(registry.perform("photo_search", intent("""{"type":"search","text":"dog"}""")).ok)

        val add = registry.perform("photo_search", intent("""{"type":"add_to_album","ids":["p1","p2"],"albumId":"al"}"""))
        assertTrue(add.ok, add.message)
        assertEquals(listOf("Added 2 photos to Holidays."), added)
        assertTrue(sent.single().startsWith("add /api/albums/al/items"), sent.toString())

        val fresh = registry.perform("photo_search", intent("""{"type":"add_to_new_album","ids":["p2"],"name":" Dogs "}"""))
        assertTrue(fresh.ok, fresh.message)
        assertTrue(sent.any { it.startsWith("create") && it.contains("\"Dogs\"") }, sent.toString())

        assertEquals("No photo \"zz\" in the results.", registry.perform("photo_search", intent("""{"type":"add_to_album","ids":["zz"],"albumId":"al"}""")).message)
        assertEquals("No album \"nope\" you can add to.", registry.perform("photo_search", intent("""{"type":"add_to_album","ids":["p1"],"albumId":"nope"}""")).message)

        val open = registry.perform("photo_search", intent("""{"type":"open","id":"p1"}"""))
        assertFalse("add_to_album" in open.screens.screen().actionNames())
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

private const val ALBUM = """{"id":"al","name":"Holidays","item_count":2,"updated_at":"2026-11-16T12:00:00Z","access":"owner"}"""
