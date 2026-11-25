package app.squirrel.ui.photos

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.explore.PhotoFilter
import app.squirrel.ui.shell.Navigator
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
import app.squirrel.ui.testing.testApi
import app.squirrel.ui.testing.text
import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
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

/** The Photos section driven the way an agent does: ls, then do, through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class PhotosLsDoTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() {
        scope.cancel()
        Dispatchers.resetMain()
    }

    private val sent = mutableListOf<String>()

    private fun album(id: String, name: String, access: String = "owner") =
        """{"id":"$id","name":"$name","item_count":2,"updated_at":"2026-11-16T12:00:00Z","access":"$access"}"""

    private suspend fun api(): SquirrelApi = testApi { request ->
        val path = request.url.encodedPath
        val post = request.method == HttpMethod.Post
        when {
            path == "/api/photos/months" -> json("""[{"month":"2024-07","count":2},{"month":"2024-06","count":1}]""")
            path == "/api/photos" && request.url.parameters["month"] == "2024-07" ->
                json("""{"items":[${summary("p1")},${summary("p2")}],"next_cursor":null}""")
            path == "/api/photos" -> json("""{"items":[${summary("p3", "2024-06-01T10:00:00")}],"next_cursor":null}""")
            path == "/api/albums" && post -> { sent += "create ${request.text()}"; json(album("new", "Trip")) }
            path == "/api/albums" -> json("[${album("al", "Holidays")}]")
            path == "/api/shared-with-me" -> json("[]")
            path == "/api/albums/al/items" && post -> { sent += "add ${request.text()}"; json("""{"changed":2}""") }
            path == "/api/albums/al/items/remove" -> { sent += "remove ${request.text()}"; json("""{"changed":1}""") }
            path.endsWith("/items") -> json("""{"items":[${summary("p1")},${summary("p2")}],"next_cursor":null}""")
            path == "/api/albums/al" && request.method == HttpMethod.Patch -> { sent += "patch ${request.text()}"; json(album("al", "Holidays")) }
            path == "/api/albums/al" -> json(album("al", "Holidays"))
            path == "/api/albums/ro" -> json(album("ro", "Theirs", access = "read"))
            else -> respondError(HttpStatusCode.NotFound)
        }
    }

    private fun intent(text: String) = Json.parseToJsonElement(text)

    private fun registry() = LsDoRegistry(settle = 50.milliseconds)

    @Test
    fun photosShowsASubTabOpensSearchAndGoesBack() = runReal {
        val nav = Navigator<PhotosRoute>(PhotosRoute.Home)
        var mode = PhotosMode.Timeline
        lateinit var photos: PhotosLsDo
        photos = PhotosLsDo(nav, mode) { mode = it; photos.shown(nav.stack.value, it) }
        val registry = registry()
        registry.register("photos", photos)

        assertTrue(registry.perform("photos", intent("""{"type":"show","mode":"albums"}""")).ok)
        assertEquals(PhotosMode.Albums, mode)

        val searching = registry.perform("photos", intent("""{"type":"search"}""")).also { photos.shown(nav.stack.value, mode) }
        assertTrue(searching.ok, searching.message)
        val screen = registry.ls("photos") ?: error("photos is listed")
        assertEquals("search", screen.state()["page"]?.jsonPrimitive?.content)
        assertEquals("albums", screen.state()["mode"]?.jsonPrimitive?.content)
        assertEquals(listOf("back"), screen.actionNames())
        assertTrue(registry.perform("photos", intent("""{"type":"show","mode":"explore"}""")).message.orEmpty().contains("not available"))

        assertTrue(registry.perform("photos", intent("""{"type":"back"}""")).ok)
        photos.shown(nav.stack.value, mode)
        assertEquals(listOf(PhotosRoute.Home), nav.stack.value)
        assertFalse("back" in registry.ls("photos")!!.actionNames())
    }

    @Test
    fun theTimelinePagesAMonthOpensAPhotoAndAddsToAnAlbum() = runReal {
        val vm = PhotosViewModel(api())
        val added = mutableListOf<String>()
        lateinit var timeline: TimelineLsDo
        timeline = TimelineLsDo(
            vm, api(), scope,
            onOpen = { timeline.overlay.shown(PhotosOverlay(viewing = it)) },
            onClose = { timeline.overlay.shown(PhotosOverlay()) },
            onAdded = { added += it },
        )
        val registry = registry()
        registry.register("timeline", timeline)
        timeline.state.await { it.months.isNotEmpty() }

        val more = registry.perform("timeline", intent("""{"type":"more"}"""))
        assertTrue(more.ok, more.message)
        assertEquals(listOf("p1", "p2"), more.screens.screen().state().ids("photos"))
        assertFalse("filter" in more.screens.screen().actionNames())

        val add = registry.perform("timeline", intent("""{"type":"add_to_album","ids":["p1","p2"],"albumId":"al"}"""))
        assertTrue(add.ok, add.message)
        assertEquals(listOf("Added 2 photos to Holidays."), added)
        assertTrue(sent.single().startsWith("add "))

        assertEquals("No album \"zz\" you can add to.", registry.perform("timeline", intent("""{"type":"add_to_album","ids":["p1"],"albumId":"zz"}""")).message)
        assertEquals("No photo \"nope\" in the timeline loaded.", registry.perform("timeline", intent("""{"type":"open","id":"nope"}""")).message)

        val open = registry.perform("timeline", intent("""{"type":"open","id":"p2"}"""))
        assertEquals("p2", open.screens.screen().state()["viewing"]?.jsonPrimitive?.content)
        val names = open.screens.screen().actionNames()
        assertTrue("close_photo" in names && "open" !in names && "add_to_album" !in names, names.toString())
    }

    @Test
    fun aTimelineFromExploreRefinesItsFilter() = runReal {
        val vm = PhotosViewModel(api(), filter = PhotoFilter(tag = "beach"))
        val filters = mutableListOf<PhotoFilter>()
        val registry = registry()
        val timeline = TimelineLsDo(vm, api(), scope, {}, {}, title = "Beach", filter = PhotoFilter(tag = "beach"), onFilter = { filters += it })
        registry.register("timeline", timeline)
        timeline.state.await { it.months.isNotEmpty() }

        val screen = registry.ls("timeline") ?: error("timeline is listed")
        assertEquals("Beach", screen.state()["title"]?.jsonPrimitive?.content)
        assertFalse("add_to_album" in screen.actionNames())

        val done = registry.perform("timeline", intent("""{"type":"filter","filter":{"tag":"beach","kind":"screenshot"}}"""))
        assertTrue(done.ok, done.message)
        assertEquals("screenshot", filters.single().kind?.value)
    }

    @Test
    fun albumsOpensAnAlbumAndCreatesOne() = runReal {
        val vm = AlbumsViewModel(api())
        val opened = mutableListOf<String>()
        val registry = registry()
        val albums = AlbumsLsDo(vm, api(), scope) { opened += it.id }
        registry.register("albums", albums)
        albums.state.await { it.albums != null && !it.loading }

        assertEquals(listOf("al"), registry.ls("albums")!!.state().ids("albums"))
        assertTrue(registry.perform("albums", intent("""{"type":"open","id":"al"}""")).ok)
        assertEquals("No album \"nope\" in the list.", registry.perform("albums", intent("""{"type":"open","id":"nope"}""")).message)
        assertEquals("Enter a name.", registry.perform("albums", intent("""{"type":"create","name":"  "}""")).message)

        val created = registry.perform("albums", intent("""{"type":"create","name":" Trip "}"""))
        assertTrue(created.ok, created.message)
        assertEquals(listOf("al", "new"), opened)
        assertTrue(sent.single().contains("\"Trip\""), sent.toString())
        assertFalse("retry" in registry.ls("albums")!!.actionNames())
    }

    @Test
    fun anOwnedAlbumIsRenamedGetsACoverAndLosesAPhoto() = runReal {
        val vm = AlbumViewModel(api(), "al")
        val notices = mutableListOf<Pair<String, Boolean>>()
        val registry = registry()
        val album = AlbumLsDo(vm, api(), "al", "Holidays", scope, {}, {}) { text, error -> notices += text to error }
        registry.register("album", album)
        album.state.await { it.access != null && it.photos.isNotEmpty() && !it.loading }

        val screen = registry.ls("album") ?: error("album is listed")
        assertEquals(listOf("p1", "p2"), screen.state().ids("photos"))
        assertTrue(screen.actionNames().containsAll(listOf("rename", "set_cover", "remove")))

        assertTrue(registry.perform("album", intent("""{"type":"rename","name":"Summer"}""")).ok)
        assertTrue(registry.perform("album", intent("""{"type":"set_cover","id":"p1"}""")).ok)
        assertTrue(registry.perform("album", intent("""{"type":"remove","ids":["p2"]}""")).ok)
        assertEquals(listOf("Cover changed." to false, "Removed 1 photo from the album." to false), notices)
        assertEquals(3, sent.size)
        assertEquals("No photo \"zz\" in this album.", registry.perform("album", intent("""{"type":"remove","ids":["zz"]}""")).message)
    }

    @Test
    fun someoneElsesAlbumOffersNoEdits() = runReal {
        val vm = AlbumViewModel(api(), "ro")
        val registry = registry()
        val album = AlbumLsDo(vm, api(), "ro", "Theirs", scope, {}, {}) { _, _ -> }
        registry.register("album", album)
        album.state.await { it.access != null && it.photos.isNotEmpty() && !it.loading }

        val names = registry.ls("album")!!.actionNames()
        assertFalse(names.any { it in listOf("rename", "set_cover", "remove") }, names.toString())
        assertTrue(registry.perform("album", intent("""{"type":"rename","name":"Mine"}""")).message.orEmpty().contains("not available"))
        assertTrue(sent.isEmpty())
    }

    private fun JsonObject.screen() = getValue("screens").jsonArray.single().jsonObject

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.ids(list: String) = getValue(list).jsonArray.map { it.jsonObject.getValue("id").jsonPrimitive.content }

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content }
}
