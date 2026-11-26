package app.squirrel.ui.sharing

import app.squirrel.ui.shell.Navigator
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.node
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
import app.squirrel.ui.testing.testApi
import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpMethod
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

/** The Shared list and its pages driven the way an agent does: ls, then do, through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class SharedLsDoTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val owner = """{"username":"anna","display_name":"Anna"}"""
    private val album = """{"id":"al","name":"Holidays","item_count":3,"updated_at":"2026-11-20T17:05:00Z","access":"write","cover":${summary("p")},"shared_by":$owner}"""
    private val bob = """{"username":"bob","display_name":"Bob"}"""
    private val carol = """{"username":"carol","display_name":"Carol"}"""
    private val left = mutableListOf<String>()

    private fun person(id: String, who: String) = """{"id":"$id","person":$who,"permission":"read","created_at":"2026-11-20T17:05:00Z"}"""

    private suspend fun vm() = SharedViewModel(testApi { request ->
        when {
            request.url.encodedPath == "/api/shared-with-me" -> json(
                "[" + listOf(
                    """{"share_id":"s1","owner":$owner,"permission":"read","shared_at":"2026-11-20T17:05:00Z","node":${node("docs", "folder", parent = null)}}""",
                    """{"share_id":"s2","owner":$owner,"permission":"write","shared_at":"2026-11-20T17:05:00Z","album":$album}""",
                    """{"share_id":"s3","owner":$owner,"permission":"read","shared_at":"2026-11-20T17:05:00Z","node":${node("tax")}}""",
                ).joinToString(",") + "]",
            )
            request.url.encodedPath == "/api/shared-by-me" -> json(
                """[{"people":[${person("m1", bob)},${person("m2", carol)}],"shared_at":"2026-11-20T17:05:00Z","node":${node("trip", "folder")}}]""",
            )
            request.url.encodedPath.startsWith("/api/shares/") && request.method == HttpMethod.Delete -> {
                left += request.url.encodedPath.substringAfterLast('/')
                respond("", HttpStatusCode.NoContent)
            }
            else -> respondError(HttpStatusCode.NotFound)
        }
    })

    private fun intent(text: String) = Json.parseToJsonElement(text)

    /** The list and the tab's stack, wired as SharedTab wires them. */
    private class Tab(vm: SharedViewModel) {
        val nav = Navigator<SharedRoute>(SharedRoute.List)
        val navLsDo = SharedNavLsDo(nav)
        val list = SharedListLsDo(vm) { item: SharedTarget ->
            item.album?.let { nav.push(SharedRoute.Album(it.id, it.name)) } ?: item.node?.let { nav.push(SharedRoute.Folder(it.id)) }
            navLsDo.shown(nav.top)
        }
    }

    @Test
    fun anAgentListsWhatIsSharedOpensAFolderAndGoesBack() = run {
        val tab = Tab(vm())
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("shared_list", tab.list)
        registry.register("shared_nav", tab.navLsDo)

        val listed = registry.perform("shared_list", intent("""{"type":"refresh"}"""))
        assertTrue(listed.ok, listed.message)
        val rows = listed.screens.screen("shared_list").state().getValue("items").jsonArray.map { it.jsonObject }
        assertEquals(listOf("s1", "s2", "s3"), rows.map { it.string("id") })
        assertEquals(listOf("folder", "album", "file"), rows.map { it.string("kind") })
        assertEquals("Anna", rows.first().string("owner"))
        assertEquals("3", rows[1].string("photoCount"))
        assertFalse("back" in listed.screens.screen("shared_nav").actionNames())

        val opened = registry.perform("shared_list", intent("""{"type":"open","id":"s1"}"""))
        assertTrue(opened.ok, opened.message)
        assertEquals(SharedRoute.Folder("docs"), tab.nav.top)
        val nav = opened.screens.screen("shared_nav")
        assertEquals("folder", nav.state().string("page"))
        assertEquals("docs", nav.state().string("id"))

        val back = registry.perform("shared_nav", intent("""{"type":"back"}"""))
        assertTrue(back.ok, back.message)
        assertEquals(SharedRoute.List, tab.nav.top)
        assertEquals("list", back.screens.screen("shared_nav").state().string("page"))
    }

    @Test
    fun aFileOrAMissingRowIsRefusedAndLeavingAsksFirst() = run {
        val tab = Tab(vm())
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("shared_list", tab.list)
        registry.perform("shared_list", intent("""{"type":"refresh"}"""))

        assertEquals("No share \"nope\" in the list.", registry.perform("shared_list", intent("""{"type":"open","id":"nope"}""")).message)
        val file = registry.perform("shared_list", intent("""{"type":"open","id":"s3"}"""))
        assertFalse(file.ok)
        assertTrue(file.message.orEmpty().contains("another app"), file.message)
        assertEquals(SharedRoute.List, tab.nav.top)

        val confirmEarly = registry.perform("shared_list", intent("""{"type":"confirm_leave"}"""))
        assertTrue(confirmEarly.message.orEmpty().contains("not available"), confirmEarly.message)

        val asked = registry.perform("shared_list", intent("""{"type":"ask_leave","id":"s2"}"""))
        assertEquals("s2", asked.screens.screen("shared_list").state().string("leaving"))
        assertTrue("confirm_leave" in asked.screens.screen("shared_list").actionNames())

        val done = registry.perform("shared_list", intent("""{"type":"confirm_leave"}"""))
        assertTrue(done.ok, done.message)
        assertEquals(listOf("s2"), left)
        val rows = done.screens.screen("shared_list").state().getValue("items").jsonArray.map { it.jsonObject.string("id") }
        assertEquals(listOf("s1", "s3"), rows)
    }

    @Test
    fun anAgentSeesWhatTheySharedOpensItAndManagesIt() = run {
        val vm = vm()
        val tab = Tab(vm)
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("shared_list", tab.list)
        registry.register("shared_nav", tab.navLsDo)

        val listed = registry.perform("shared_list", intent("""{"type":"refresh"}"""))
        val mine = listed.screens.screen("shared_list").state().getValue("sharedByMe").jsonArray.map { it.jsonObject }
        assertEquals(listOf("trip"), mine.map { it.string("id") })
        assertEquals(listOf("Bob", "Carol"), mine.single().getValue("people").jsonArray.map { it.jsonPrimitive.content })
        assertTrue("manage" in listed.screens.screen("shared_list").actionNames())

        assertEquals("Nothing of yours \"nope\" in sharedByMe.", registry.perform("shared_list", intent("""{"type":"manage","id":"nope"}""")).message)
        val managed = registry.perform("shared_list", intent("""{"type":"manage","id":"trip"}"""))
        assertTrue(managed.ok, managed.message)
        assertEquals("trip", managed.screens.screen("shared_list").state().string("managing"))
        assertFalse("manage" in managed.screens.screen("shared_list").actionNames())
        vm.doneManaging()

        val opened = registry.perform("shared_list", intent("""{"type":"open_mine","id":"trip"}"""))
        assertTrue(opened.ok, opened.message)
        assertEquals(SharedRoute.Folder("trip"), tab.nav.top)
    }

    @Test
    fun theNavRefusesBackOnTheList() = run {
        val registry = LsDoRegistry(settle = 10.milliseconds)
        registry.register("shared_nav", SharedNavLsDo(Navigator(SharedRoute.List)))

        val screen = registry.ls("shared_nav") ?: error("shared_nav is listed")
        assertEquals(listOf("back"), screen.getValue("unavailable").jsonArray.map { it.jsonPrimitive.content })
        assertTrue(registry.perform("shared_nav", intent("""{"type":"back"}""")).message.orEmpty().contains("not available"))
    }

    private fun run(block: suspend () -> Unit) = runReal(block)

    private fun JsonObject.screen(id: String) = getValue("screens").jsonArray.map { it.jsonObject }.single { it.string("id") == id }

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.string(key: String) = getValue(key).jsonPrimitive.content

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.string("name") }
}
