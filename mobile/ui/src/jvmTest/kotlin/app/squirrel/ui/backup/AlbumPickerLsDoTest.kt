package app.squirrel.ui.backup

import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.db.createJvmDriver
import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.settings.SettingsStore
import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withContext
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

/** The album choice driven the way an agent does, through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class AlbumPickerLsDoTest {
    private val store = SettingsStore(createDatabase(createJvmDriver()))
    private var schedules = 0
    private val folders = listOf(
        AlbumChoice("-1739773001", "Camera", 120, "DCIM/Camera"),
        AlbumChoice("54321", "Screenshots", 8, "Pictures/Screenshots"),
    )

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun registry(): LsDoRegistry {
        val vm = AlbumPickerViewModel(
            store = store,
            applySchedule = { schedules++ },
            loadAlbums = { folders },
            kind = AlbumKind.Folders,
            io = Dispatchers.Unconfined,
        )
        return LsDoRegistry(settle = 50.milliseconds).apply { register("album_picker", AlbumPickerLsDo(vm)) }
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun anAgentOpensThePickerAndChoosesAFolder() = run {
        val registry = registry()

        val opened = registry.perform("album_picker", intent("""{"type":"open"}"""))
        assertTrue(opened.ok, opened.message)
        val sheet = opened.screens.screen()
        assertEquals("folders", sheet.state().getValue("kind").jsonPrimitive.content)
        assertEquals(listOf("Camera", "Screenshots"), sheet.state().getValue("albums").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content })
        assertTrue("toggle" in sheet.actionNames() && "open" !in sheet.actionNames())

        val chosen = registry.perform("album_picker", intent("""{"type":"toggle","id":"54321"}"""))
        assertTrue(chosen.ok, chosen.message)
        assertEquals(AlbumFilter.Selected(setOf("54321")), store.backup().albums)
        val state = chosen.screens.screen().state()
        assertEquals("Screenshots", state.getValue("label").jsonPrimitive.content)
        assertEquals("false", state.getValue("all").jsonPrimitive.content)

        assertTrue(registry.perform("album_picker", intent("""{"type":"select_all"}""")).ok)
        assertEquals(AlbumFilter.All, store.backup().albums)
        assertEquals(2, schedules)
        val closed = registry.perform("album_picker", intent("""{"type":"close"}"""))
        assertEquals("false", closed.screens.screen().state().getValue("open").jsonPrimitive.content)
    }

    @Test
    fun aClosedSheetOnlyOpensAndAnUnknownFolderIsRefused() = run {
        val registry = registry()

        val closed = registry.ls("album_picker") ?: error("album_picker is listed")
        assertEquals(listOf("open"), closed.actionNames())
        assertFalse(registry.perform("album_picker", intent("""{"type":"toggle","id":"54321"}""")).ok)

        registry.perform("album_picker", intent("""{"type":"open"}"""))
        assertEquals("No folder \"nope\" on this phone.", registry.perform("album_picker", intent("""{"type":"toggle","id":"nope"}""")).message)
        assertEquals(AlbumFilter.All, store.backup().albums)
        assertEquals(0, schedules)
    }

    private fun JsonObject.screen() = getValue("screens").jsonArray.single().jsonObject

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content }
}
