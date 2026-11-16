package app.squirrel.shared.sync

import app.cash.turbine.test
import app.squirrel.shared.db.ScannedAsset
import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.db.recordScanned
import app.squirrel.shared.settings.SettingsStore
import app.squirrel.shared.testing.testDriver
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

class BackupStatusTest {
    private val nbsp = " "

    @Test
    fun countsGroupThousands() {
        assertEquals("7", formatCount(7))
        assertEquals("1${nbsp}204", formatCount(1204))
        assertEquals("1${nbsp}204${nbsp}000", formatCount(1_204_000))
        assertEquals("1${nbsp}204 left", BackupStatus(left = 1204).leftLabel())
        assertEquals("Everything is backed up", BackupStatus(total = 3, backedUp = 3).leftLabel())
    }

    @Test
    fun sizesUseDecimalUnitsLikeTheWeb() {
        assertEquals("0 B", formatBytes(0))
        assertEquals("999 B", formatBytes(999))
        assertEquals("1.5 MB", formatBytes(1_500_000))
        assertEquals("3.2 GB", formatBytes(3_210_000_000))
        assertEquals("120 GB", formatBytes(120_000_000_000))
        assertEquals("100.0 MB", formatBytes(99_960_000))
        assertEquals("1000 GB", formatBytes(999_999_999_999)) // as the web: toFixed(0) of 999.99
    }

    @Test
    fun lastRunIsRelative() {
        val now = 10_000_000_000L
        assertEquals("Not backed up yet", formatLastRun(null, now))
        assertEquals("Backed up just now", formatLastRun(now - 30_000, now))
        assertEquals("Backed up 2 min ago", formatLastRun(now - 2 * 60_000, now))
        assertEquals("Backed up 3 h ago", formatLastRun(now - 3 * 3_600_000, now))
        assertEquals("Backed up yesterday", formatLastRun(now - 30 * 3_600_000, now))
        assertEquals("Backed up 4 days ago", formatLastRun(now - 4 * 86_400_000L, now))
    }

    @Test
    fun theStatusFollowsTheQueueAndTheEngine() = runTest {
        val db = createDatabase(testDriver())
        val queue = Queue(db)
        val settings = SettingsStore(db)
        val engineState = MutableStateFlow<SyncState>(SyncState.Idle)
        fun scan(id: String, size: Long) =
            db.recordScanned(ScannedAsset(id, "photo", "$id.jpg", null, size, 1, null, null), 1)
        scan("images:1", 1_000_000)
        scan("images:2", 2_200_000)

        backupStatus(queue, engineState, settings, access = { MediaAccess.Partial(2) }).test {
            val first = awaitItem()
            assertEquals(2, first.total)
            assertEquals(2, first.left)
            assertEquals(3_200_000, first.bytesLeft)
            assertEquals(MediaAccess.Partial(2), first.access)

            queue.setHash("images:1", "h")
            queue.markPresent(listOf("images:1"))
            var next = awaitItem()
            while (next.backedUp != 1L) next = awaitItem()
            assertEquals(1, next.left)

            engineState.value = SyncState.Paused(PauseReason.WifiRequired)
            next = awaitItem()
            while (next.paused == null) next = awaitItem()
            assertEquals(PauseReason.WifiRequired, next.paused)

            settings.setValue(LAST_RUN_KEY, "1234")
            next = awaitItem()
            while (next.lastRun == null) next = awaitItem()
            assertEquals(1234L, next.lastRun)
            cancelAndIgnoreRemainingEvents()
        }
    }
}
