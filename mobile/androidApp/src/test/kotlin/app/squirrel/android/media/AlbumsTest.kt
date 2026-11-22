package app.squirrel.android.media

import android.provider.MediaStore
import app.squirrel.ui.backup.AlbumChoice
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class AlbumsTest {
    @Test
    fun sameNamedAlbumsAreToldApartByVolume() {
        val albums = labelled(
            listOf(
                Album("1", "Camera", MediaStore.VOLUME_EXTERNAL_PRIMARY, 120),
                Album("2", "Camera", "1a2b-3c4d", 30),
                Album("3", "Screenshots", MediaStore.VOLUME_EXTERNAL_PRIMARY, 8),
            ),
        )
        assertEquals(listOf("Camera (phone)", "Camera (SD card)", "Screenshots"), albums.map { it.label })
        assertEquals(listOf("1", "2", "3"), albums.map { it.id })
    }

    @Test
    fun theCameraFolderShowsWhereItLives() {
        val camera = Album("-1739773001", "Camera", MediaStore.VOLUME_EXTERNAL_PRIMARY, 120, path = "DCIM/Camera/")
        // Same id (the filter's key, unchanged), the bucket's own name, and its path.
        assertEquals(AlbumChoice("-1739773001", "Camera", 120, "DCIM/Camera"), camera.choice())
    }

    @Test
    fun theChoiceUsesTheLabelThatToldTwinsApart() {
        val twins = labelled(
            listOf(
                Album("1", "Camera", MediaStore.VOLUME_EXTERNAL_PRIMARY, 120, path = "DCIM/Camera/"),
                Album("2", "Camera", "1a2b-3c4d", 30, path = "DCIM/Camera/"),
            ),
        ).map { it.choice() }
        assertEquals(listOf("Camera (phone)", "Camera (SD card)"), twins.map { it.name })
        assertEquals(listOf("DCIM/Camera", "DCIM/Camera"), twins.map { it.path })
    }

    @Test
    fun pathsLoseTheirTrailingSlashAndBlanksAreDropped() {
        assertEquals("Pictures/WhatsApp Images", folderPath("Pictures/WhatsApp Images/"))
        assertEquals("Download", folderPath("Download"))
        assertNull(folderPath(null))
        assertNull(folderPath("/"))
        assertNull(folderPath("  "))
    }
}
