package app.squirrel.ui.notifications

import app.squirrel.ui.testing.runReal
import io.github.axellaffite.lsdo.runtime.LsDoRegistry
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

/** The bell and its sheet driven the way an agent does. */
@OptIn(ExperimentalCoroutinesApi::class)
class NotificationsLsDoTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun anAgentOpensTheSheetPagesAndOpensAnItem() = runReal {
        val vm = NotificationsViewModel(NotificationsServer().api())
        val went = mutableListOf<NotificationTarget>()
        val registry = LsDoRegistry(settle = 50.milliseconds)
        registry.register("notifications", NotificationsLsDo(vm) { went += it })

        val opened = registry.perform("notifications", intent("""{"type":"open"}"""))
        assertTrue(opened.ok, opened.message)
        val screen = opened.screens.screen("notifications")
        val rows = screen.state().getValue("items").jsonArray.map { it.jsonObject }
        assertEquals(listOf("n1", "n2"), rows.map { it.string("id") })
        assertEquals("shared “Trip” with you · can edit", rows.first().string("text"))
        assertEquals("trip", rows[1].string("folderId"))
        assertTrue("more" in screen.actionNames())

        val more = registry.perform("notifications", intent("""{"type":"more"}"""))
        assertTrue(more.ok, more.message)
        assertFalse("more" in more.screens.screen("notifications").actionNames())

        val file = registry.perform("notifications", intent("""{"type":"open_item","id":"n3"}"""))
        assertFalse(file.ok)
        assertTrue(file.message.orEmpty().contains("another app"), file.message)

        val album = registry.perform("notifications", intent("""{"type":"open_item","id":"n4"}"""))
        assertTrue(album.ok, album.message)
        assertEquals(listOf<NotificationTarget>(NotificationTarget.Album("al", "Holidays")), went)
        val after = album.screens.screen("notifications").state()
        assertEquals("false", after.string("open"))
        assertEquals("3", after.string("unread"))
    }

    private fun JsonObject.screen(id: String) = getValue("screens").jsonArray.map { it.jsonObject }.single { it.string("id") == id }

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.string(key: String) = getValue(key).jsonPrimitive.content

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.string("name") }
}
