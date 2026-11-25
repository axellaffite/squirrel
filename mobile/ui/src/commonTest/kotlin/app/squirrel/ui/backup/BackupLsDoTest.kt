package app.squirrel.ui.backup

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.BackupStatus
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.shared.sync.QueueState
import app.squirrel.shared.sync.QueuedAsset
import app.squirrel.shared.sync.SyncState
import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
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

/** The Backup screen driven the way an agent does: ls, then do, through the LS-DO registry. */
@OptIn(ExperimentalCoroutinesApi::class)
class BackupLsDoTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val status = MutableStateFlow(BackupStatus(total = 10, backedUp = 7, left = 2, failed = 1, bytesLeft = 4_000_000))
    private var access: MediaAccess = MediaAccess.Full
    private var runs = 0
    private var settingsOpened = 0

    private val failure = QueuedAsset(
        deviceAssetId = "42", kind = "photo", name = "IMG_0042.jpg", mime = "image/jpeg", size = 1000, modifiedAt = 0,
        takenAt = null, album = null, sha256 = null, state = QueueState.Failed, uploadId = null, attempts = 3,
        nextAttemptAt = 0, lastError = "Server said 500",
    )

    private fun registry(): LsDoRegistry {
        val vm = BackupViewModel(
            status = status,
            settings = MutableStateFlow(BackupSettings()),
            locationsIncluded = { true },
            access = { access },
            now = { 0L },
            onBackUpNow = { runs++ },
            failures = { listOf(failure) },
        )
        return LsDoRegistry(settle = 50.milliseconds).apply { register("backup", BackupLsDo(vm) { settingsOpened++ }) }
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private fun intent(type: String) = Json.parseToJsonElement("""{"type":"$type"}""")

    @Test
    fun anAgentReadsTheFiguresThenRetriesWhatFailed() = run {
        val registry = registry()
        registry.perform("backup", intent("refresh"))

        val shown = registry.perform("backup", intent("show_failures"))
        assertTrue(shown.ok, shown.message)
        val screen = shown.screens.screen()
        val state = screen.state()
        assertEquals(7, state.getValue("backedUp").jsonPrimitive.content.toInt())
        assertEquals(2, state.getValue("waiting").jsonPrimitive.content.toInt())
        assertEquals("IMG_0042.jpg", state.getValue("failures").jsonArray.single().jsonObject.getValue("name").jsonPrimitive.content)
        assertTrue("retry_failures" in screen.actionNames() && "show_failures" !in screen.actionNames())

        val retried = registry.perform("backup", intent("retry_failures"))
        assertTrue(retried.ok, retried.message)
        assertEquals(1, runs)
        assertEquals("null", retried.screens.screen().state().getValue("failures").toString())

        assertTrue(registry.perform("backup", intent("open_settings")).ok)
        assertEquals(1, settingsOpened)
    }

    @Test
    fun aRunningBackupCannotBeStartedAgain() = run {
        status.value = status.value.copy(state = SyncState.Uploading("IMG_1.jpg", done = 1, of = 4))
        val registry = registry()
        registry.perform("backup", intent("refresh"))

        val screen = registry.ls("backup") ?: error("backup is listed")
        assertEquals("uploading", screen.state().getValue("step").jsonPrimitive.content)
        assertEquals(0.25f, screen.state().getValue("progress").jsonPrimitive.content.toFloat())
        assertFalse("back_up_now" in screen.actionNames())
        val refused = registry.perform("backup", intent("back_up_now"))
        assertFalse(refused.ok)
        assertTrue(refused.message.orEmpty().contains("not available"), refused.message)
        assertEquals(0, runs)
    }

    @Test
    fun withoutPhotoAccessOnlyRefreshIsOffered() = run {
        access = MediaAccess.None
        val registry = registry()
        registry.perform("backup", intent("refresh"))

        val screen = registry.ls("backup") ?: error("backup is listed")
        assertEquals("none", screen.state().getValue("access").jsonPrimitive.content)
        assertEquals(listOf("refresh"), screen.actionNames())
        assertFalse(registry.perform("backup", intent("open_settings")).ok)

        // Access granted in the system dialog, then the app comes back.
        access = MediaAccess.Full
        val back = registry.perform("backup", intent("refresh"))
        assertTrue("back_up_now" in back.screens.screen().actionNames())
    }

    private fun JsonObject.screen() = getValue("screens").jsonArray.single().jsonObject

    private fun JsonObject.state() = getValue("state").jsonObject

    private fun JsonObject.actionNames() = getValue("actions").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content }
}
