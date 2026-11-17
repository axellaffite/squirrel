package app.squirrel.android.media

import android.Manifest.permission.ACCESS_MEDIA_LOCATION
import android.Manifest.permission.POST_NOTIFICATIONS
import android.Manifest.permission.READ_EXTERNAL_STORAGE
import android.Manifest.permission.READ_MEDIA_IMAGES
import android.Manifest.permission.READ_MEDIA_VIDEO
import android.Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED
import app.squirrel.shared.sync.MediaAccess
import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class MediaPermissionsTest {
    @Test
    fun theRequestedSetFitsTheApiLevel() {
        assertEquals(listOf(READ_EXTERNAL_STORAGE, ACCESS_MEDIA_LOCATION), MediaPermissions.toRequest(29))
        assertEquals(listOf(READ_EXTERNAL_STORAGE, ACCESS_MEDIA_LOCATION), MediaPermissions.toRequest(32))
        assertEquals(
            listOf(READ_MEDIA_IMAGES, READ_MEDIA_VIDEO, ACCESS_MEDIA_LOCATION, POST_NOTIFICATIONS),
            MediaPermissions.toRequest(33),
        )
        assertEquals(
            listOf(READ_MEDIA_IMAGES, READ_MEDIA_VIDEO, READ_MEDIA_VISUAL_USER_SELECTED, ACCESS_MEDIA_LOCATION, POST_NOTIFICATIONS),
            MediaPermissions.toRequest(35),
        )
    }

    private fun access(sdk: Int, vararg granted: String) =
        MediaPermissions.access(sdk, granted = { it in granted }, selectedCount = { 42 })

    @Test
    fun accessPerGrantCombination() {
        assertEquals(MediaAccess.Full, access(32, READ_EXTERNAL_STORAGE))
        assertEquals(MediaAccess.None, access(32))
        assertEquals(MediaAccess.Full, access(33, READ_MEDIA_IMAGES, READ_MEDIA_VIDEO))
        assertEquals(MediaAccess.None, access(33, READ_EXTERNAL_STORAGE))
        assertEquals(MediaAccess.NoVideos, access(33, READ_MEDIA_IMAGES))
        assertEquals(MediaAccess.NoVideos, access(35, READ_MEDIA_IMAGES, READ_MEDIA_VISUAL_USER_SELECTED))
        assertEquals(MediaAccess.Full, access(34, READ_MEDIA_IMAGES, READ_MEDIA_VIDEO, READ_MEDIA_VISUAL_USER_SELECTED))
        // "Select photos" on Android 14+: only the user-selected grant.
        assertEquals(MediaAccess.Partial(42), access(34, READ_MEDIA_VISUAL_USER_SELECTED))
        assertEquals(MediaAccess.Partial(42), access(35, READ_MEDIA_VISUAL_USER_SELECTED))
        assertEquals(MediaAccess.None, access(35))
    }

    @Test
    fun locationsDependOnMediaLocation() {
        assertTrue(MediaPermissions.locationsIncluded { it == ACCESS_MEDIA_LOCATION })
        assertFalse(MediaPermissions.locationsIncluded { false })
    }
}
