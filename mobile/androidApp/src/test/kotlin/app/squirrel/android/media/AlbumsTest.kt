package app.squirrel.android.media

import android.provider.MediaStore
import org.junit.Test
import kotlin.test.assertEquals

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
}
