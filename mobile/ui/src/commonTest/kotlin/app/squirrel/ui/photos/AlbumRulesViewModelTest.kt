package app.squirrel.ui.photos

import app.squirrel.api.models.AlbumRulesDto
import app.squirrel.api.models.MatchMode
import app.squirrel.ui.testing.apiError
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
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

@OptIn(ExperimentalCoroutinesApi::class)
class AlbumRulesViewModelTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val previews = mutableListOf<String>()
    private val saved = mutableListOf<Pair<HttpMethod, String>>()

    private fun album(id: String, name: String) =
        """{"id":"$id","name":"$name","item_count":3,"updated_at":"2026-11-20T17:05:00Z","access":"owner","dynamic":true,"rules":{"tags":["beach"]}}"""

    private suspend fun api() = testApi { request ->
        when (request.url.encodedPath) {
            "/api/people" -> json("""[{"id":"anna","name":"Anna","hidden":false,"face_count":3},{"id":"ghost","hidden":true,"face_count":1}]""")
            "/api/tags" -> json("""[{"tag":"beach","count":2,"cover":${summary("q")}}]""")
            "/api/places" -> json("""[{"country_code":"FR","country":"France","count":4,"cover":${summary("p")}}]""")
            "/api/cameras" -> json("""[{"make":"Canon","model":"R6","count":9}]""")
            "/api/albums/preview" -> {
                val body = request.text()
                previews += body
                if ("\"nope\"" in body) apiError(HttpStatusCode.ServiceUnavailable, "busy", "Too slow, try fewer rules.")
                else json("""{"count":7,"newest":[${summary("n1")}]}""")
            }
            "/api/albums" -> { saved += request.method to request.text(); json(album("new", "Beach"), HttpStatusCode.Created) }
            "/api/albums/al" -> { saved += request.method to request.text(); json(album("al", "Beach")) }
            else -> respondError(HttpStatusCode.NotFound)
        }
    }

    @Test
    fun choicesLoadAndHiddenPeopleStayOut() = runReal {
        val vm = AlbumRulesViewModel(api(), albumId = null, debounceMillis = 0)
        val state = vm.state.await { it.choicesLoaded }
        assertEquals(listOf("anna"), state.people.map { it.id })
        assertEquals("beach", state.tags.single().tag)
        assertEquals("FR", state.places.single().countryCode)
        assertEquals("Canon", state.cameras.single().make)
        assertNull(state.preview)
        assertTrue(previews.isEmpty(), "nothing to preview without a rule")
    }

    @Test
    fun editingPreviewsTheLatestRulesOnly() = runReal {
        val vm = AlbumRulesViewModel(api(), albumId = null, debounceMillis = 50)
        vm.edit { it.toggleTag("beach") }
        vm.edit { it.togglePerson("anna") }
        val state = vm.state.await { it.preview != null && !it.previewing }
        assertEquals(7L, state.preview?.count)
        assertEquals(1, previews.size, "the debounce sends one preview for both edits")
        assertTrue("\"anna\"" in previews.single() && "\"beach\"" in previews.single())
    }

    @Test
    fun aRefusedPreviewSaysWhy() = runReal {
        val vm = AlbumRulesViewModel(api(), albumId = null, debounceMillis = 0)
        vm.edit { it.toggleTag("nope") }
        assertEquals("Too slow, try fewer rules.", vm.state.await { it.previewError != null }.previewError)
    }

    @Test
    fun creatingNeedsANameThenPostsTheRules() = runReal {
        val vm = AlbumRulesViewModel(api(), albumId = null, debounceMillis = 0)
        vm.edit { it.toggleTag("beach") }
        assertNull(vm.save())
        assertEquals("Enter a name.", vm.state.value.error)
        assertTrue(saved.isEmpty())
        vm.setName(" Beach ")
        assertEquals("new", vm.save()?.id)
        val (method, body) = saved.single()
        assertEquals(HttpMethod.Post, method)
        assertTrue("\"name\":\"Beach\"" in body && "\"tags\":[\"beach\"]" in body, body)
    }

    @Test
    fun editingStartsFromTheSavedRulesAndPatchesThem() = runReal {
        val vm = AlbumRulesViewModel(api(), albumId = "al", initial = AlbumRulesDto(tags = listOf("beach"), tagsMatch = MatchMode.any), debounceMillis = 0)
        assertEquals(7L, vm.state.await { it.preview != null }.preview?.count)
        vm.edit { it.copy(tagsMatch = MatchMode.all) }
        assertEquals("al", vm.save()?.id)
        val (method, body) = saved.single()
        assertEquals(HttpMethod.Patch, method)
        assertTrue("\"tags_match\":\"all\"" in body && "\"name\"" !in body, body)
    }
}
