package app.squirrel.lsdo.runtime

import app.squirrel.lsdo.LsDoEntry
import app.squirrel.lsdo.lsdoSpec
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class CatalogTest {
    private val entries = listOf(
        LsDoEntry("counter", "A counter. Reached by: opening it.", lsdoSpec<CounterState, CounterIntent>()),
        LsDoEntry("other", "Another one.", lsdoSpec<CounterState, CounterIntent>()),
    )

    @Test
    fun `the index lists every screen with its action names and whether it is shown`() {
        val screens = catalogIndex(entries, shown = setOf("counter")).getValue("screens").jsonArray.map { it.jsonObject }

        assertEquals(listOf("counter", "other"), screens.map { it.getValue("name").jsonPrimitive.content })
        assertEquals(listOf("add", "fail", "reset"), screens.first().getValue("actions").jsonArray.map { it.jsonPrimitive.content })
        assertEquals(listOf(true, false), screens.map { it.getValue("shown").jsonPrimitive.boolean })
    }

    @Test
    fun `a screen in full has each action's doc and params and its state's shape`() {
        val screen = catalogScreen(entries.first())

        val add = screen.getValue("actions").jsonArray.first().jsonObject
        assertEquals("Adds n to the count", add.getValue("description").jsonPrimitive.content)
        assertTrue("n" in add.getValue("params").jsonObject.getValue("properties").jsonObject)
        assertEquals("The current count", screen.getValue("state").jsonObject.getValue("properties").jsonObject.getValue("count").jsonObject.getValue("description").jsonPrimitive.content)
    }
}
