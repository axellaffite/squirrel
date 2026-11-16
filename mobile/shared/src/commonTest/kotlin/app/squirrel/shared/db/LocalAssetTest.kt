package app.squirrel.shared.db

import app.squirrel.shared.testing.testDriver
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class LocalAssetTest {
    private val db = createDatabase(testDriver())
    private val q = db.localAssetQueries

    private fun scan(id: String, size: Long = 100, modified: Long = 1, generation: Long = 1) =
        db.recordScanned(
            ScannedAsset(id, kind = "photo", name = "$id.jpg", mime = "image/jpeg", size = size,
                modifiedAt = modified, takenAt = "2024-07-14T18:30:05+02:00", album = "Camera"),
            generation,
        )

    private fun row(id: String) = q.byId(id).executeAsOne()

    @Test
    fun aFirstScanInsertsANewAsset() {
        scan("images:1")
        val asset = row("images:1")
        assertEquals("new", asset.state)
        assertEquals("2024-07-14T18:30:05+02:00", asset.taken_at)
        assertNull(asset.sha256)
    }

    @Test
    fun anUnchangedFileKeepsItsHashAndState() {
        scan("images:1")
        q.setHash("h1", null, null, "images:1")
        q.markPresent(listOf("images:1"))

        scan("images:1", generation = 2)

        assertEquals("h1", row("images:1").sha256)
        assertEquals("done", row("images:1").state)
        assertEquals(2, row("images:1").scan_generation)
    }

    @Test
    fun aChangedFileIsHashedAndCheckedAgain() {
        scan("images:1")
        q.setHash("h1", null, null, "images:1")
        q.markMissing(listOf("images:1"))
        q.markUploading("u1", "images:1")

        scan("images:1", size = 200)

        val asset = row("images:1")
        assertNull(asset.sha256)
        assertNull(asset.upload_id)
        assertEquals("new", asset.state)
    }

    @Test
    fun aSkippedAssetSelectedAgainResumes() {
        scan("images:1")
        scan("images:2")
        q.setHash("h1", null, null, "images:1")
        q.skipUnseen(2)

        scan("images:1", generation = 3)
        scan("images:2", generation = 3)

        assertEquals("hashed", row("images:1").state)
        assertEquals("new", row("images:2").state)
    }

    @Test
    fun countsGroupByState() {
        scan("images:1")
        scan("images:2")
        scan("images:3", size = 50)
        q.setHash("h", null, null, "images:1")
        q.markPresent(listOf("images:1"))

        assertEquals(mapOf("new" to 2L, "done" to 1L), db.counts())
        assertEquals(150L, q.bytesLeft().executeAsOne())
    }

    @Test
    fun aChangedFileForgetsWhatWasHashed() {
        scan("images:1", size = 100)
        q.setHash("h1", 99, 5, "images:1")
        assertEquals(99, row("images:1").hashed_size)

        scan("images:1", size = 200)

        assertNull(row("images:1").hashed_size)
        assertNull(row("images:1").hashed_modified_at)
    }

    @Test
    fun forgettingTheDeviceKeepsOnlyTheAddressAndPreferences() {
        scan("images:1")
        db.scanStateQueries.put("phone", "token", 1, 0)
        listOf("server.url", "backup.wifi_only", "backup.last_run", "account.display_name", "scan.filter")
            .forEach { db.settingQueries.put(it, "x") }

        db.forgetDevice()

        assertEquals(emptyMap(), db.counts())
        assertNull(db.scanStateQueries.get("phone").executeAsOneOrNull())
        assertEquals(listOf("backup.wifi_only", "server.url"), db.settingQueries.all().executeAsList().map { it.key }.sorted())
    }
}
