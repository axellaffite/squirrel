package app.squirrel.ui.explore

import app.squirrel.lsdo.runtime.LsDoRegistry
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
import app.squirrel.ui.testing.testApi
import io.ktor.client.engine.mock.respondError
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

@OptIn(ExperimentalCoroutinesApi::class)
class ExploreLsDoTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() {
        scope.cancel()
        Dispatchers.resetMain()
    }

    private val people = (1..7).joinToString(",", "[", "]") { """{"id":"p$it","name":"Person $it","hidden":false,"face_count":$it}""" }

    private suspend fun vm() = ExploreViewModel(testApi { request ->
        when (request.url.encodedPath) {
            "/api/people" -> json(people)
            "/api/places" -> json("""[{"country_code":"FR","country":"France","region":"Bretagne","city":"Brest","count":4,"cover":${summary("a")}}]""")
            "/api/tags" -> json("""[{"tag":"beach","count":2,"cover":${summary("b")}}]""")
            "/api/cameras" -> json("""[{"make":"Canon","model":"R6","count":9}]""")
            else -> respondError(HttpStatusCode.NotFound)
        }
    }).also { it.state.await { s -> s.loaded && !s.loading } }

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun theOverviewOpensACardAndSeesAllOfASection() = runReal {
        val opened = mutableListOf<Pair<PhotoFilter, String>>()
        val seeAll = mutableListOf<ExploreSection>()
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("explore", ExploreLsDo(vm(), null, scope, { seeAll += it }) { filter, title -> opened += filter to title })

        val screen = registry.ls("explore") ?: error("explore is listed")
        assertEquals(6, screen.state().getValue("people").jsonArray.size)
        assertEquals(listOf("people"), screen.state().getValue("more").jsonArray.map { it.jsonPrimitive.content })
        assertEquals(listOf("place:FR/Bretagne/Brest"), screen.state().ids("places"))

        assertTrue(registry.perform("explore", intent("""{"type":"open","id":"place:FR/Bretagne/Brest"}""")).ok)
        assertTrue(registry.perform("explore", intent("""{"type":"open","id":"camera:Canon/R6"}""")).ok)
        assertEquals(
            listOf(PhotoFilter(country = "FR", region = "Bretagne", city = "Brest") to "Brest", PhotoFilter(cameraMake = "Canon", cameraModel = "R6") to "Canon R6"),
            opened,
        )
        assertEquals("No card \"person:p7\" shown on Explore.", registry.perform("explore", intent("""{"type":"open","id":"person:p7"}""")).message)

        assertTrue(registry.perform("explore", intent("""{"type":"see_all","section":"people"}""")).ok)
        assertEquals(listOf(ExploreSection.People), seeAll)
        assertEquals("Tags has no more to show.", registry.perform("explore", intent("""{"type":"see_all","section":"tags"}""")).message)
    }

    @Test
    fun aSectionPageListsAllOfItAndNothingElse() = runReal {
        val opened = mutableListOf<String>()
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("explore", ExploreLsDo(vm(), ExploreSection.People, scope, {}) { _, title -> opened += title })

        val screen = registry.ls("explore") ?: error("explore is listed")
        assertEquals(7, screen.state().getValue("people").jsonArray.size)
        assertTrue(screen.state().getValue("tags").jsonArray.isEmpty())
        assertFalse("see_all" in screen.getValue("actions").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content })

        assertTrue(registry.perform("explore", intent("""{"type":"open","id":"person:p7"}""")).ok)
        assertEquals(listOf("Person 7"), opened)
        assertEquals("No card \"tag:beach\" shown on Explore.", registry.perform("explore", intent("""{"type":"open","id":"tag:beach"}""")).message)
    }

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.ids(list: String) = getValue(list).jsonArray.map { it.jsonObject.getValue("id").jsonPrimitive.content }
}
