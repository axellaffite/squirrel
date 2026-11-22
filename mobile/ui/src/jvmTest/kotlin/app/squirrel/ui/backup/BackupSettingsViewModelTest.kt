package app.squirrel.ui.backup

import app.squirrel.shared.api.ApiError
import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.db.createJvmDriver
import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.settings.SettingsStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse

@OptIn(ExperimentalCoroutinesApi::class)
class BackupSettingsViewModelTest {
    private val store = SettingsStore(createDatabase(createJvmDriver()))
    private var schedules = 0
    private val renamed = mutableListOf<String>()
    private var renameFailure: Exception? = null

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun vm() = BackupSettingsViewModel(
        store = store,
        applySchedule = { schedules++ },
        rename = { name -> renameFailure?.let { throw it }; renamed += name; name },
        deviceName = "Pixel 8",
        io = Dispatchers.Unconfined,
    )

    @Test
    fun eachTogglePersistsAndReschedules() {
        val vm = vm()
        vm.setAutoBackup(false)
        vm.setWifiOnly(false)
        vm.setChargingOnly(true)
        vm.setIncludeVideos(false)

        val saved = store.backup()
        assertFalse(saved.autoBackup)
        assertFalse(saved.wifiOnly)
        assertEquals(true, saved.chargingOnly)
        assertFalse(saved.includeVideos)
        assertEquals(4, schedules)
    }

    @Test
    fun aToggleKeepsTheFoldersChosenElsewhere() {
        val vm = vm()
        // The picker (Backup screen) saves the folder choice after this screen loaded.
        store.setBackup(store.backup().copy(albums = AlbumFilter.Selected(setOf("-1739773001"))))
        vm.setWifiOnly(false)
        assertEquals(AlbumFilter.Selected(setOf("-1739773001")), store.backup().albums)
        assertFalse(store.backup().wifiOnly)
    }

    @Test
    fun renamingValidatesOneTo64Characters() {
        val vm = vm()
        vm.onNameChange("   ")
        vm.saveName()
        assertEquals("Enter a name for this phone", vm.state.value.nameError)
        vm.onNameChange("x".repeat(65))
        vm.saveName()
        assertEquals("Use at most 64 characters", vm.state.value.nameError)
        assertEquals(emptyList(), renamed)

        vm.onNameChange("  Ada's phone ")
        vm.saveName()
        assertEquals(listOf("Ada's phone"), renamed)
        assertEquals("Backups go to Photos/Ada's phone/<year>", vm.state.value.folderHint)
    }

    @Test
    fun aRefusedNameShowsTheServersMessage() {
        renameFailure = ApiError(422, "validation_error", "That name is already used by another phone.")
        val vm = vm()
        vm.onNameChange("Pixel")
        vm.saveName()
        assertEquals("That name is already used by another phone.", vm.state.value.nameError)
    }
}
