package app.squirrel.shared.settings

import app.cash.turbine.test
import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.testing.testDriver
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class BackupSettingsTest {
    private val store = SettingsStore(createDatabase(testDriver()))

    @Test
    fun defaultsAreWifiOnlyWithVideosAndAllAlbums() {
        assertEquals(
            BackupSettings(wifiOnly = true, chargingOnly = false, includeVideos = true, albums = AlbumFilter.All),
            store.backup(),
        )
    }

    @Test
    fun settingsRoundTrip() {
        val custom = BackupSettings(
            wifiOnly = false,
            chargingOnly = true,
            includeVideos = false,
            albums = AlbumFilter.Selected(setOf("Camera", "Screenshots, 2024")),
        )
        store.setBackup(custom)
        assertEquals(custom, store.backup())
    }

    @Test
    fun theServerAddressIsStored() {
        assertNull(store.serverUrl())
        store.setServerUrl("https://home.example")
        assertEquals("https://home.example", store.serverUrl())
        store.setServerUrl(null)
        assertNull(store.serverUrl())
    }

    @Test
    fun changesAreObservable() = runTest {
        store.backupFlow().test {
            assertEquals(true, awaitItem().wifiOnly)
            store.setBackup(store.backup().copy(wifiOnly = false))
            assertEquals(false, awaitItem().wifiOnly)
        }
    }
}
