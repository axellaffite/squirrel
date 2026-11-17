package app.squirrel.ui.backup

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.BackupStatus
import app.squirrel.shared.sync.ErrorReason
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.shared.sync.PauseReason
import app.squirrel.shared.sync.SyncState
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class BackupUiTest {
    private val nbsp = " "
    private val now = 10_000_000_000L
    private val status = BackupStatus(total = 1300, backedUp = 96, left = 1204, bytesLeft = 3_210_000_000, lastRun = now - 120_000)

    private fun ui(
        s: BackupStatus = status,
        settings: BackupSettings = BackupSettings(),
        locations: Boolean = true,
    ) = backupUi(s, settings, locations, now)

    @Test
    fun idleShowsWhatIsLeft() {
        val ui = ui()
        assertEquals("1${nbsp}204 left", ui.featured)
        assertEquals("3.2 GB · Wi-Fi only", ui.detail)
        assertEquals("Backed up 2 min ago", ui.lastRun)
        assertEquals("Back up now", ui.button)
        assertFalse(ui.running)
        assertEquals("96", ui.backedUp)
        assertEquals("1${nbsp}204", ui.waiting)
        assertNull(ui.notice)
        assertNull(ui.progress)
        assertEquals(AccessUi.Full, ui.access)
    }

    @Test
    fun everythingBackedUp() {
        assertEquals("Everything is backed up", ui(BackupStatus(total = 5, backedUp = 5)).featured)
        assertEquals("0 B · Any network", ui(BackupStatus(), BackupSettings(wifiOnly = false)).detail)
        assertEquals("Ready to back up", ui(BackupStatus()).featured)
    }

    @Test
    fun uploadingShowsProgress() {
        val ui = ui(status.copy(state = SyncState.Uploading("IMG_0042.jpg", done = 3, of = 12)))
        assertTrue(ui.running)
        assertEquals("Backing up…", ui.button)
        assertEquals(0.25f, ui.progress)
        assertEquals("Uploading IMG_0042.jpg", ui.progressLabel)
    }

    @Test
    fun theOtherStepsSayWhatTheyDo() {
        assertEquals("Looking for new photos", ui(status.copy(state = SyncState.Scanning)).progressLabel)
        assertEquals("Preparing 40 items", ui(status.copy(state = SyncState.Hashing(10, 40))).progressLabel)
        assertEquals("Checking what your Squirrel already has", ui(status.copy(state = SyncState.Checking)).progressLabel)
    }

    @Test
    fun pausesAreSaidInPlainWords() {
        val texts = PauseReason.entries.associateWith { ui(status.copy(state = SyncState.Paused(it))).notice }
        assertEquals("Waiting for Wi-Fi", texts[PauseReason.WifiRequired])
        assertEquals("Waiting for charging", texts[PauseReason.ChargingRequired])
        assertEquals("Waiting for the battery to charge", texts[PauseReason.BatteryLow])
        assertEquals("Waiting for a network connection", texts[PauseReason.NoNetwork])
        assertEquals("Squirrel cannot see your photos", texts[PauseReason.NoPhotoAccess])
        assertEquals("Storage full: free some space on your Squirrel to continue", texts[PauseReason.StorageFull])
        assertTrue(ui(status.copy(state = SyncState.Paused(PauseReason.StorageFull))).noticeIsError)
        assertFalse(ui(status.copy(state = SyncState.Paused(PauseReason.WifiRequired))).noticeIsError)
    }

    @Test
    fun errorsAreShown() {
        val ui = ui(status.copy(state = SyncState.Error(ErrorReason.Unreachable)))
        assertEquals("Your Squirrel could not be reached. Backup will try again.", ui.notice)
        assertTrue(ui.noticeIsError)
        assertEquals("Backup stopped because of an error. It will try again.", ui(status.copy(state = SyncState.Error(ErrorReason.Unexpected))).notice)
    }

    @Test
    fun failuresAreCounted() {
        val ui = ui(status.copy(failed = 2))
        assertEquals("2", ui.failed)
        assertEquals(2, ui.failedCount)
    }

    @Test
    fun partialAccessIsShownHonestly() {
        assertEquals(
            AccessUi.Partial("Squirrel can see 42 selected photos. Only these are backed up."),
            ui(status.copy(access = MediaAccess.Partial(42))).access,
        )
        assertEquals(
            AccessUi.Partial("Squirrel can see 1 selected photo. Only these are backed up."),
            ui(status.copy(access = MediaAccess.Partial(1))).access,
        )
        assertEquals(AccessUi.None, ui(status.copy(access = MediaAccess.None)).access)
    }

    @Test
    fun missingLocationPermissionIsWarned() {
        assertEquals("Photo locations won't be backed up", ui(locations = false).locationWarning)
        assertNull(ui(locations = true).locationWarning)
        assertNull(ui(status.copy(access = MediaAccess.None), locations = false).locationWarning)
    }
}
