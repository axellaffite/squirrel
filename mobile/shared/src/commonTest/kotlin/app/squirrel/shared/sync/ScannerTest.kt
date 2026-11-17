package app.squirrel.shared.sync

import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.settings.SettingsStore
import app.squirrel.shared.testing.FakeAssetSource
import app.squirrel.shared.testing.testDriver
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

class ScannerTest {
    private val db = createDatabase(testDriver())
    private val queue = Queue(db)
    private val settings = SettingsStore(db)
    private val source = FakeAssetSource()
    private val scanner = Scanner(source, db, queue, settings)

    private fun state(id: String) = queue.get(id)?.state

    @Test
    fun aFirstScanInsertsEveryAsset() = runTest {
        repeat(3) { source.put("images:$it") }

        val summary = scanner.scan()

        assertEquals(ScanSummary(seen = 3, removed = 0, full = true), summary)
        assertEquals(mapOf(QueueState.New to 3L), queue.counts())
    }

    @Test
    fun aSecondScanAppliesOnlyTheChanges() = runTest {
        source.put("images:1")
        source.put("images:2")
        scanner.scan()
        queue.setHash("images:1", "h1")
        queue.markPresent(listOf("images:1"))

        source.put("images:3")
        source.remove("images:2")
        val summary = scanner.scan()

        assertEquals(ScanSummary(seen = 1, removed = 1, full = false), summary)
        assertEquals(QueueState.Done, state("images:1"))
        assertEquals(QueueState.Skipped, state("images:2"))
        assertEquals(QueueState.New, state("images:3"))
    }

    @Test
    fun changingTheAlbumFilterRescansAndSkipsWhatIsNoLongerSelected() = runTest {
        source.put("images:1", album = "Camera")
        source.put("images:2", album = "WhatsApp")
        scanner.scan()
        queue.setHash("images:2", "h2")
        queue.markPresent(listOf("images:2"))

        settings.setBackup(BackupSettings(albums = AlbumFilter.Selected(setOf("WhatsApp"))))
        val summary = scanner.scan()

        assertEquals(true, summary.full)
        assertEquals(QueueState.Skipped, state("images:1"))
        // Already backed up: stays done (never deleted from the server).
        assertEquals(QueueState.Done, state("images:2"))
    }

    @Test
    fun withoutVideosVideosAreSkipped() = runTest {
        settings.setBackup(BackupSettings(includeVideos = false))
        source.put("images:1")
        source.put("video:1", kind = "video")

        scanner.scan()

        assertEquals(QueueState.New, state("images:1"))
        assertEquals(null, state("video:1"))

        settings.setBackup(BackupSettings(includeVideos = true))
        scanner.scan()
        assertEquals(QueueState.New, state("video:1"))
    }

    @Test
    fun aModifiedFileIsQueuedAgain() = runTest {
        source.put("images:1", size = 100)
        scanner.scan()
        queue.setHash("images:1", "h1")
        queue.markPresent(listOf("images:1"))

        source.put("images:1", size = 200, modifiedAt = 2)
        scanner.scan()

        assertEquals(QueueState.New, state("images:1"))
    }

    @Test
    fun aPermissionChangeForcesAFullScan() = runTest {
        source.put("images:1")
        scanner.scan()
        source.put("images:2") // visible before, but the token moved on: only a rescan finds it again
        scanner.scan()

        source.fingerprint = "partial|4f2a"
        val summary = scanner.scan()

        assertEquals(true, summary.full)
        assertEquals(2, summary.seen)
        assertEquals("partial|4f2a", settings.value(SCAN_ACCESS_KEY))
        assertEquals(false, scanner.scan().full) // and incremental again afterwards
    }

    @Test
    fun reconcilingSkipsWhatVanishedButKeepsWhatIsDone() = runTest {
        repeat(4) { source.put("images:$it") }
        scanner.scan()
        queue.setHash("images:0", "h0")
        queue.markPresent(listOf("images:0")) // backed up
        queue.setHash("images:1", "h1")
        queue.applyCheck(listOf("images:1"), missing = listOf("images:1"))
        queue.markFailed("images:1", "boom")
        listOf("images:0", "images:1", "images:2").forEach(source::remove) // deleted on the phone

        assertEquals(2, scanner.reconcile())

        assertEquals(QueueState.Done, state("images:0")) // the server copy stays
        assertEquals(QueueState.Skipped, state("images:1"))
        assertEquals(QueueState.Skipped, state("images:2"))
        assertEquals(QueueState.New, state("images:3"))
    }
}
