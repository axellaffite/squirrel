package app.squirrel.lsdo.runtime

import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.seconds

class LsDoRegistryTest {
    private fun intent(text: String) = LsDoJson.parseToJsonElement(text)

    @Test
    fun `ls lists each screen with its state and the actions available now`() {
        val registry = LsDoRegistry()
        registry.register("counter", Counter())

        val screen = registry.ls().screens().single()

        assertEquals("counter", screen["id"]?.jsonPrimitive?.content)
        assertEquals(0, screen["state"]?.jsonObject?.get("count")?.jsonPrimitive?.int)
        assertEquals(listOf("add", "fail"), screen.actionNames())
    }

    @Test
    fun `a second screen with the same name gets its own id and leaves when unregistered`() {
        val registry = LsDoRegistry()
        registry.register("counter", Counter())
        val remove = registry.register("counter", Counter())

        assertEquals(listOf("counter", "counter-2"), registry.ids)
        remove()
        assertEquals(listOf("counter"), registry.ids)
    }

    @Test
    fun `do runs the intent and answers with the screens after it`() = runTest {
        val registry = LsDoRegistry()
        registry.register("counter", Counter())

        val outcome = registry.perform("counter", intent("""{"type":"add","n":3}"""))

        assertTrue(outcome.ok)
        val screen = outcome.screens.screens().single()
        assertEquals(3, screen["state"]?.jsonObject?.get("count")?.jsonPrimitive?.int)
        assertEquals(listOf("add", "fail", "reset"), screen.actionNames())
    }

    @Test
    fun `refusals unknown actions unavailable actions and crashes come back as messages`() = runTest {
        val registry = LsDoRegistry()
        registry.register("counter", Counter())

        assertEquals("Only positive numbers.", registry.perform("counter", intent("""{"type":"add","n":-1}""")).message)
        assertTrue(registry.perform("counter", intent("""{"type":"jump"}""")).message.orEmpty().startsWith("Not an action"))
        assertTrue(registry.perform("counter", intent("""{"type":"add"}""")).message.orEmpty().startsWith("Not an action"))
        assertTrue(registry.perform("counter", intent("""{"type":"reset"}""")).message.orEmpty().contains("not available"))
        assertEquals("\"fail\" failed: boom", registry.perform("counter", intent("""{"type":"fail"}""")).message)
        assertTrue(registry.perform("nope", intent("""{"type":"add","n":1}""")).message.orEmpty().startsWith("No screen"))
    }

    @Test
    fun `do waits while the screen is busy`() = runTest {
        val registry = LsDoRegistry()
        val counter = Counter()
        registry.register("counter", counter)
        counter.setLoading(true)
        launch {
            advanceTimeBy(2_000)
            counter.setLoading(false)
        }

        val outcome = registry.perform("counter", intent("""{"type":"add","n":1}"""))

        assertFalse(outcome.screens.screens().single()["busy"]?.jsonPrimitive?.content.toBoolean())
        assertTrue(testScheduler.currentTime >= 2_000)
    }

    @Test
    fun `a screen still busy at the timeout says so`() = runTest {
        val registry = LsDoRegistry(timeout = 1.seconds)
        val counter = Counter()
        registry.register("counter", counter)
        counter.setLoading(true)

        val outcome = registry.perform("counter", intent("""{"type":"add","n":1}"""))

        assertTrue(outcome.ok)
        assertTrue(outcome.message.orEmpty().startsWith("Still busy"))
    }

    @Test
    fun `ls of an unknown screen is null`() {
        assertNull(LsDoRegistry().ls("nope"))
    }

    private fun JsonObject.screens() = getValue("screens").jsonArray.map { it.jsonObject }

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content }
}
