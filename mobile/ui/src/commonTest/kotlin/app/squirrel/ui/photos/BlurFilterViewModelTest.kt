package app.squirrel.ui.photos

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
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class BlurFilterViewModelTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val puts = mutableListOf<String>()
    private var changed = 0

    /** [stored]: what `GET /api/me/blur-filter` answers. */
    private suspend fun api(stored: String = """{"enabled":false,"rules":null}""") = testApi { request ->
        when (request.url.encodedPath) {
            "/api/people" -> json("[]")
            "/api/tags" -> json("""[{"tag":"beach","count":2,"cover":${summary("q")}}]""")
            "/api/places", "/api/cameras" -> json("[]")
            "/api/albums/preview" -> json("""{"count":3,"newest":[${summary("n1")}]}""")
            "/api/me/blur-filter" -> when (request.method) {
                HttpMethod.Get -> json(stored)
                HttpMethod.Put -> {
                    val body = request.text()
                    puts += body
                    if ("\"refused\"" in body) apiError(HttpStatusCode.UnprocessableEntity, "invalid_rules", "Too many tags.")
                    else json(body)
                }
                else -> respondError(HttpStatusCode.MethodNotAllowed)
            }
            else -> respondError(HttpStatusCode.NotFound)
        }
    }

    private suspend fun vm(stored: String = """{"enabled":false,"rules":null}""") =
        AlbumRulesViewModel(api(stored), RulesTarget.BlurFilter, debounceMillis = 0, onFilterChanged = { changed++ })

    @Test
    fun withoutAFilterTheEditorStartsEmptyAndEnabled() = runReal {
        val state = vm().state.await { it.filter?.loading == false }
        val filter = checkNotNull(state.filter)
        assertTrue(filter.enabled, "a new filter blurs once saved")
        assertNull(filter.saved)
        assertFalse(state.creating, "no name to ask")
        assertTrue(state.problem != null, "nothing to save without a rule")
    }

    @Test
    fun aSavedFilterFillsTheEditorAndItsCount() = runReal {
        val state = vm("""{"enabled":true,"rules":{"tags":["beach"]}}""").state.await { it.preview != null }
        assertEquals(listOf("beach"), state.form.tags)
        assertEquals(listOf("beach"), state.filter?.saved?.tags)
        assertEquals(3L, state.preview?.count)
    }

    @Test
    fun savingPutsTheRulesAndRefreshesTheBlurredPhotos() = runReal {
        val vm = vm()
        vm.state.await { it.filter?.loading == false }
        assertFalse(vm.saveFilter(), "no rule yet")
        assertTrue(puts.isEmpty())
        vm.edit { it.toggleTag("beach") }
        assertTrue(vm.saveFilter())
        val body = puts.single()
        assertTrue("\"enabled\":true" in body && "\"tags\":[\"beach\"]" in body, body)
        assertEquals(1, changed)
        val filter = checkNotNull(vm.state.value.filter)
        assertEquals(listOf("beach"), filter.saved?.tags)
        assertEquals("Blur filter saved.", filter.notice)
    }

    @Test
    fun aRefusedSaveSaysWhyAndChangesNothing() = runReal {
        val vm = vm()
        vm.state.await { it.filter?.loading == false }
        vm.edit { it.toggleTag("refused") }
        assertFalse(vm.saveFilter())
        assertEquals("Too many tags.", vm.state.value.error)
        assertNull(vm.state.value.filter?.saved)
        assertEquals(0, changed)
    }

    @Test
    fun theSwitchAppliesAtOnceWithTheSavedRulesNotTheEditedOnes() = runReal {
        val vm = vm("""{"enabled":true,"rules":{"tags":["beach"]}}""")
        vm.state.await { it.filter?.loading == false }
        vm.edit { it.toggleTag("sea") }
        assertTrue(vm.setFilterEnabled(false))
        val body = puts.single()
        assertTrue("\"enabled\":false" in body && "\"tags\":[\"beach\"]" in body && "sea" !in body, body)
        assertFalse(checkNotNull(vm.state.value.filter).enabled)
        assertEquals(1, changed)
    }

    @Test
    fun theSwitchWaitsForTheFirstSaveWithoutAFilter() = runReal {
        val vm = vm()
        vm.state.await { it.filter?.loading == false }
        assertTrue(vm.setFilterEnabled(false))
        assertTrue(puts.isEmpty())
        vm.edit { it.toggleTag("beach") }
        vm.saveFilter()
        assertTrue("\"enabled\":false" in puts.single(), puts.single())
    }

    @Test
    fun removingForgetsTheRules() = runReal {
        val vm = vm("""{"enabled":true,"rules":{"tags":["beach"]}}""")
        vm.state.await { it.filter?.loading == false }
        assertTrue(vm.removeFilter())
        val body = puts.single()
        assertTrue("\"enabled\":false" in body && "\"rules\"" !in body, body)
        val state = vm.state.value
        assertNull(state.filter?.saved)
        assertTrue(state.form.tags.isEmpty())
        assertEquals(1, changed)
        assertFalse(vm.removeFilter(), "nothing left to remove")
    }

    @Test
    fun albumSaveDoesNothingOnTheFilter() = runReal {
        val vm = vm()
        vm.state.await { it.filter?.loading == false }
        vm.edit { it.toggleTag("beach") }
        assertNull(vm.save())
        assertTrue(puts.isEmpty())
    }
}
