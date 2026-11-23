package app.squirrel.ui.explore

import app.squirrel.api.models.FaceBoxDto
import app.squirrel.api.models.FaceCrop
import app.squirrel.api.models.PhotoKindFilter
import app.squirrel.ui.photos.PhotosViewModel
import app.squirrel.ui.photos.UNREACHABLE
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
import app.squirrel.ui.testing.testApi
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode
import io.ktor.http.Parameters
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
class ExploreViewModelTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val people = """[{"id":"anna","name":"Anna","hidden":false,"face_count":3,"cover":{"face_id":"f","photo_id":"p","thumbnail_version":2,"box":{"x":0.4,"y":0.4,"width":0.1,"height":0.2}}}]"""
    private val places = """[{"country_code":"FR","country":"France","region":null,"city":"Brest","count":4,"cover":${summary("p")}}]"""
    private val tags = """[{"tag":"beach","count":2,"cover":${summary("q")}}]"""
    private val cameras = """[{"make":"Canon","model":"R6","count":9}]"""

    @Test
    fun everyListArrivesTogether() = runReal {
        val vm = ExploreViewModel(testApi { request ->
            when (request.url.encodedPath) {
                "/api/people" -> json(people)
                "/api/places" -> json(places).also { assertEquals("city", request.url.parameters["level"]) }
                "/api/tags" -> json(tags)
                "/api/cameras" -> json(cameras)
                else -> respondError(HttpStatusCode.NotFound)
            }
        })
        val state = vm.state.await { it.loaded && !it.loading }
        assertEquals("Anna", state.people?.single()?.name)
        assertEquals("Brest", state.places?.single()?.city)
        assertEquals("beach", state.tags?.single()?.tag)
        assertEquals("R6", state.cameras?.single()?.model)
        assertNull(state.error)
        assertEquals(false, state.empty)
    }

    @Test
    fun aFailedListIsEmptyAndOffersTryAgain() = runReal {
        var down = true
        val vm = ExploreViewModel(testApi { request ->
            when (request.url.encodedPath) {
                "/api/tags" -> if (down) respondError(HttpStatusCode.BadGateway) else json(tags)
                "/api/people" -> json("[]")
                "/api/places" -> json("[]")
                "/api/cameras" -> json("[]")
                else -> respondError(HttpStatusCode.NotFound)
            }
        })
        val failed = vm.state.await { it.loaded && !it.loading }
        assertEquals(UNREACHABLE, failed.error)
        assertTrue(failed.empty)
        down = false
        vm.load()
        val again = vm.state.await { !it.loading && it.error == null }
        assertEquals(listOf("beach"), again.tags?.map { it.tag })
    }

    @Test
    fun aFilteredTimelineAsksForTheFilterEverywhere() = runReal {
        val asked = mutableListOf<Pair<String, Parameters>>()
        val vm = PhotosViewModel(testApi { request ->
            asked += request.url.encodedPath to request.url.parameters
            when (request.url.encodedPath) {
                "/api/photos/months" -> json("""[{"month":"2024-07","count":1}]""")
                "/api/photos" -> json("""{"items":[${summary("p")}],"next_cursor":null}""")
                else -> respondError(HttpStatusCode.NotFound)
            }
        }, filter = PhotoFilter(country = "FR", region = "", city = "Brest", kind = PhotoKindFilter.photo))
        vm.state.await { it.months.isNotEmpty() }
        vm.ensure("2024-07")
        assertEquals(listOf("p"), vm.state.await { it.loaded().isNotEmpty() }.loaded().map { it.id })
        asked.forEach { (_, params) ->
            assertEquals("FR", params["country"])
            assertEquals("", params["region"])
            assertEquals("Brest", params["city"])
            assertEquals("photo", params["kind"])
        }
        assertEquals("2024-07", asked.last().second["month"])
    }

    @Test
    fun thePickerOffersWhatIsNotHiddenAndMarksTheChoice() {
        val choices = FilterChoices(emptyList(), emptyList(), listOf(app.squirrel.api.models.CameraGroupDto(count = 1, make = "Canon", model = null)), failed = false)
        val hidden = app.squirrel.api.models.PersonDto(faceCount = 1, hidden = true, id = "h")
        val anna = app.squirrel.api.models.PersonDto(faceCount = 2, hidden = false, id = "anna", name = "Anna")
        val sections = filterSections(PhotoFilter(person = "anna", cameraMake = "Canon"), listOf(hidden, anna), choices)
        assertEquals(listOf("People", "Cameras", "Kind"), sections.map { it.first })
        assertEquals(listOf("Anna"), sections[0].second.map { it.label })
        assertTrue(sections[0].second.single().chosen)
        assertTrue(sections[1].second.single().chosen)
        assertEquals(PhotoKindFilter.screenshot, sections[2].second[1].next.kind)
        assertEquals("anna", sections[2].second[1].next.person)
    }

    @Test
    fun smallFacesCropThePreview() = runReal {
        val api = testApi { respondError(HttpStatusCode.NotFound) }
        val big = FaceCrop(box = FaceBoxDto(height = 0.5f, width = 0.3f, x = 0.3f, y = 0.2f), faceId = "f", photoId = "p", thumbnailVersion = 3)
        assertEquals("https://home.example/api/files/p/thumbnail?size=thumb&v=3", cropSource(api, big, displayPx = 112.0))
        val small = big.copy(box = FaceBoxDto(height = 0.05f, width = 0.03f, x = 0.3f, y = 0.2f))
        assertEquals("https://home.example/api/files/p/thumbnail?size=preview&v=3", cropSource(api, small, displayPx = 112.0))
        assertNull(cropSource(api, big.copy(thumbnailVersion = null), displayPx = 112.0))
    }
}
