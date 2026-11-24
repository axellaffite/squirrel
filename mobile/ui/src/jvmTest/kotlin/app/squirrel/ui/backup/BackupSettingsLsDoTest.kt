package app.squirrel.ui.backup

import app.squirrel.lsdo.runtime.LsDoRegistry
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.db.createJvmDriver
import app.squirrel.shared.settings.SettingsStore
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
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

/** Backup › Settings driven the way an agent does, through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class BackupSettingsLsDoTest {
    private val store = SettingsStore(createDatabase(createJvmDriver()))
    private var schedules = 0
    private var backs = 0
    private var renameFailure: Exception? = null
    private var renameGate: CompletableDeferred<Unit>? = null

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun registry(): LsDoRegistry {
        val vm = BackupSettingsViewModel(
            store = store,
            applySchedule = { schedules++ },
            rename = { name ->
                renameGate?.await()
                renameFailure?.let { throw it }
                name
            },
            deviceName = "Pixel 8",
            io = Dispatchers.Unconfined,
        )
        return LsDoRegistry(settle = 50.milliseconds).apply { register("backup_settings", BackupSettingsLsDo(vm) { backs++ }) }
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private fun intent(text: String) = Json.parseToJsonElement(text)

    @Test
    fun anAgentTurnsWifiOnlyOffRenamesThePhoneAndGoesBack() = run {
        val registry = registry()

        val toggled = registry.perform("backup_settings", intent("""{"type":"set_wifi_only","on":false}"""))
        assertTrue(toggled.ok, toggled.message)
        assertFalse(store.backup().wifiOnly)
        assertEquals(1, schedules)
        assertEquals("false", toggled.screens.screen().state().getValue("wifiOnly").jsonPrimitive.content)

        val renamed = registry.perform("backup_settings", intent("""{"type":"rename","name":"  Ada's phone "}"""))
        assertTrue(renamed.ok, renamed.message)
        val state = renamed.screens.screen().state()
        assertEquals("Ada's phone", state.getValue("savedName").jsonPrimitive.content)
        assertEquals("Backups go to Photos/Ada's phone/<year>", state.getValue("folderHint").jsonPrimitive.content)

        assertTrue(registry.perform("backup_settings", intent("""{"type":"back"}""")).ok)
        assertEquals(1, backs)
    }

    @Test
    fun aBadOrRefusedNameIsTheRefusal() = run {
        val registry = registry()

        assertEquals("Enter a name for this phone", registry.perform("backup_settings", intent("""{"type":"rename","name":"  "}""")).message)
        renameFailure = ApiError(422, "validation_error", "That name is already used by another phone.")
        val refused = registry.perform("backup_settings", intent("""{"type":"rename","name":"Pixel"}"""))
        assertFalse(refused.ok)
        assertEquals("That name is already used by another phone.", refused.message)
    }

    @Test
    fun renameIsNotOfferedWhileANameIsSaving() = run {
        val registry = registry()
        val gate = CompletableDeferred<Unit>()
        renameGate = gate

        coroutineScope {
            val saving = async { registry.perform("backup_settings", intent("""{"type":"rename","name":"Pixel 9"}""")) }
            val busy = generateSequence { registry.ls("backup_settings") }.first { it.getValue("busy").jsonPrimitive.content == "true" }
            assertFalse("rename" in busy.actionNames())
            gate.complete(Unit)
            assertTrue(saving.await().ok)
        }
        assertTrue("rename" in (registry.ls("backup_settings") ?: error("listed")).actionNames())
    }

    private fun JsonObject.screen() = getValue("screens").jsonArray.single().jsonObject

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content }
}
