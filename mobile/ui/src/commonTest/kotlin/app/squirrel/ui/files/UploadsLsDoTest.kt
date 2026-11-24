package app.squirrel.ui.files

import app.squirrel.lsdo.runtime.LsDoRegistry
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
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
import kotlin.test.assertTrue
import kotlin.time.Duration.Companion.milliseconds

/** Uploads listed and dismissed the way an agent does, through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class UploadsLsDoTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun anAgentSeesUploadsAndDismissesOnlyAFailedOne() = runReal {
        val vm = UploadsViewModel(testApi { respondError(HttpStatusCode.NotFound) })
        val registry = LsDoRegistry(settle = 20.milliseconds)
        registry.register("uploads", vm)
        assertEquals(listOf("dismiss"), registry.ls("uploads")?.getValue("unavailable")?.jsonArray?.map { it.jsonPrimitive.content })

        // The first cannot be read, so it fails; the second then uploads for as long as the test runs.
        vm.enqueue(listOf(PickedFile("lost.txt", open = { null }), PickedFile("big.mov", open = { awaitCancellation() })), parentId = "docs")
        vm.state.await { it.failed == 1 }

        val screen = registry.ls("uploads") ?: error("uploads is listed")
        val state = screen.getValue("state").jsonObject
        assertEquals("1", state.string("uploading"))
        val items = state.getValue("items").jsonArray.map { it.jsonObject }
        assertEquals(listOf("lost.txt", "big.mov"), items.map { it.string("name") })
        assertEquals(listOf("docs", "docs"), items.map { it.string("parentId") })
        assertTrue(items.first().string("error").contains("could not be read"))

        assertEquals("No upload 9 in the list.", registry.perform("uploads", intent("""{"type":"dismiss","key":9}""")).message)
        val running = registry.perform("uploads", intent("""{"type":"dismiss","key":1}"""))
        assertTrue(running.message.orEmpty().contains("still uploading"), running.message)

        val dismissed = registry.perform("uploads", intent("""{"type":"dismiss","key":0}"""))
        assertTrue(dismissed.ok, dismissed.message)
        val left = dismissed.screens.getValue("screens").jsonArray.single().jsonObject.getValue("state").jsonObject
        assertEquals(listOf("big.mov"), left.getValue("items").jsonArray.map { it.jsonObject.string("name") })
        assertEquals("0", left.string("failed"))
    }

    private fun JsonObject.string(key: String) = getValue(key).jsonPrimitive.content
}
