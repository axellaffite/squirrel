package app.squirrel.ui.backup

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
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class AlbumPickerViewModelTest {
    private val store = SettingsStore(createDatabase(createJvmDriver()))
    private var schedules = 0
    private var loads = 0
    private val folders = listOf(
        AlbumChoice("-1739773001", "Camera", 120, "DCIM/Camera"),
        AlbumChoice("54321", "Screenshots", 8, "Pictures/Screenshots"),
    )

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun vm() = AlbumPickerViewModel(
        store = store,
        applySchedule = { schedules++ },
        loadAlbums = { loads++; folders },
        kind = AlbumKind.Folders,
        io = Dispatchers.Unconfined,
    )

    @Test
    fun theChoiceIsKeptByIdAndShownByName() {
        val vm = vm()
        assertEquals("All photos", vm.state.value.label)
        vm.open()
        assertTrue(vm.state.value.open)
        assertEquals(listOf("Camera", "Screenshots"), vm.state.value.albums?.map { it.name })

        vm.toggle("-1739773001")
        assertEquals(AlbumFilter.Selected(setOf("-1739773001")), store.backup().albums) // by id, as before
        assertEquals("Camera", vm.state.value.label)
        vm.toggle("54321")
        assertEquals("Camera, Screenshots", vm.state.value.label)
        vm.toggle("-1739773001")
        assertEquals(AlbumFilter.Selected(setOf("54321")), store.backup().albums)
        vm.selectAll()
        assertEquals(AlbumFilter.All, store.backup().albums)
        assertEquals(4, schedules)
        vm.close()
        assertFalse(vm.state.value.open)
    }

    @Test
    fun itStartsFromTheStoredChoiceAndReloadsFoldersWhenOpened() {
        store.setBackup(store.backup().copy(albums = AlbumFilter.Selected(setOf("54321"))))
        val vm = vm()
        assertEquals("Screenshots", vm.state.value.label) // names loaded for the row
        vm.open()
        assertEquals(2, loads) // fresh folders each time the sheet opens
    }

    @Test
    fun savingTheChoiceKeepsTheOtherSettings() {
        store.setBackup(store.backup().copy(wifiOnly = false, includeVideos = false))
        val vm = vm()
        vm.toggle("54321")
        val saved = store.backup()
        assertFalse(saved.wifiOnly)
        assertFalse(saved.includeVideos)
        assertEquals(AlbumFilter.Selected(setOf("54321")), saved.albums)
    }
}
