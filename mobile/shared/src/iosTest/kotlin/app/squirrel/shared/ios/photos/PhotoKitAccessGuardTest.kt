package app.squirrel.shared.ios.photos

import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.sync.SourceFilter
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** The test runner never has photo access: every call must answer without touching PhotoKit. */
class PhotoKitAccessGuardTest {
    private val source = PhotoKitSource(AssetExporter("/nonexistent/export"))

    @Test
    fun withoutAccessNothingReachesPhotoKit() = runTest {
        assertFalse(PhotoAccess.granted())
        val scan = source.changes("token-1", SourceFilter(includeVideos = false, albums = AlbumFilter.All))
        assertTrue(scan.assets.isEmpty())
        assertEquals("token-1", scan.token) // the change position is kept for later
        assertNull(source.open("asset-1"))
        assertEquals(setOf("asset-1"), source.existing(listOf("asset-1"))) // never reported deleted
        assertTrue(source.albums().isEmpty())
    }
}
