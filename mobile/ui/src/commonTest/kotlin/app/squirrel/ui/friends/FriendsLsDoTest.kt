package app.squirrel.ui.friends

import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import app.squirrel.ui.sharing.SharedNavLsDo
import app.squirrel.ui.sharing.SharedRoute
import app.squirrel.ui.shell.Navigator
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
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

/** The Friends screen driven the way an agent does: ls, then do, through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class FriendsLsDoTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val server = FriendsServer()

    private suspend fun registry(): LsDoRegistry {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("friends", FriendsLsDo(FriendsViewModel(testApi(server.handler()))))
        return registry
    }

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun anAgentAddsByUsernameAndFromSuggestionsThenRemoves() = runReal {
        val registry = registry()
        val listed = registry.perform("friends", intent("""{"type":"refresh"}"""))
        assertTrue(listed.ok, listed.message)
        val state = listed.screens.screen("friends").state()
        assertEquals(listOf("anna"), state.rows("friends").map { it.string("username") })
        assertEquals("Anna", state.rows("friends").single().string("name"))
        assertEquals(listOf("bob", "cleo"), state.rows("suggestions").map { it.string("username") })

        val added = registry.perform("friends", intent("""{"type":"add","username":"dan"}"""))
        assertTrue(added.ok, added.message)
        assertEquals(listOf("anna", "dan"), added.screens.screen("friends").state().rows("friends").map { it.string("username") })
        assertEquals("", added.screens.screen("friends").state().string("username"))

        val picked = registry.perform("friends", intent("""{"type":"add_suggestion","username":"bob"}"""))
        assertTrue(picked.ok, picked.message)
        assertEquals(listOf("cleo"), picked.screens.screen("friends").state().rows("suggestions").map { it.string("username") })

        val removed = registry.perform("friends", intent("""{"type":"remove","username":"anna"}"""))
        assertTrue(removed.ok, removed.message)
        assertEquals(listOf("bob", "dan"), removed.screens.screen("friends").state().rows("friends").map { it.string("username") })
        assertEquals(listOf("dan", "bob"), server.added)
    }

    @Test
    fun refusalsComeBackAsTheServersMessage() = runReal {
        val registry = registry()
        registry.perform("friends", intent("""{"type":"refresh"}"""))
        val refused = registry.perform("friends", intent("""{"type":"add","username":"nobody"}"""))
        assertFalse(refused.ok)
        assertEquals("No account is called nobody.", refused.message)
        assertEquals("No account is called nobody.", refused.screens.screen("friends").state().string("addError"))

        assertEquals("\"zed\" is not a suggestion.", registry.perform("friends", intent("""{"type":"add_suggestion","username":"zed"}""")).message)
        assertEquals("\"zed\" is not a friend.", registry.perform("friends", intent("""{"type":"remove","username":"zed"}""")).message)
    }

    @Test
    fun removeAndSuggestionsAreUnavailableWhenEmpty() = runReal {
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("friends", FriendsLsDo(FriendsViewModel(testApi(FriendsServer(friends = emptyList(), suggestions = emptyList()).handler()))))
        val listed = registry.perform("friends", intent("""{"type":"refresh"}"""))
        val screen = listed.screens.screen("friends")
        assertEquals(setOf("add_suggestion", "remove"), screen.getValue("unavailable").jsonArray.map { it.jsonPrimitive.content }.toSet())
        assertEquals(emptyList(), screen.state().rows("friends"))
    }

    @Test
    fun theSharedListOpensFriendsAndBackReturns() = runReal {
        val nav = Navigator<SharedRoute>(SharedRoute.List)
        val registry = LsDoRegistry(settle = 10.milliseconds)
        registry.register("shared_nav", SharedNavLsDo(nav))

        val opened = registry.perform("shared_nav", intent("""{"type":"friends"}"""))
        assertTrue(opened.ok, opened.message)
        assertEquals(SharedRoute.Friends, nav.top)
        assertEquals("friends", opened.screens.screen("shared_nav").state().string("page"))
        assertFalse("friends" in opened.screens.screen("shared_nav").actionNames())

        val back = registry.perform("shared_nav", intent("""{"type":"back"}"""))
        assertTrue(back.ok, back.message)
        assertEquals(SharedRoute.List, nav.top)
    }

    private fun JsonObject.screen(id: String) = getValue("screens").jsonArray.map { it.jsonObject }.single { it.string("id") == id }

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.rows(key: String) = getValue(key).jsonArray.map { it.jsonObject }

    private fun JsonObject.string(key: String) = getValue(key).jsonPrimitive.content

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.string("name") }
}
