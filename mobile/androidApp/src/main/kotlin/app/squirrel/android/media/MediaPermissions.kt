package app.squirrel.android.media

import android.Manifest.permission.ACCESS_MEDIA_LOCATION
import android.Manifest.permission.POST_NOTIFICATIONS
import android.Manifest.permission.READ_EXTERNAL_STORAGE
import android.Manifest.permission.READ_MEDIA_IMAGES
import android.Manifest.permission.READ_MEDIA_VIDEO
import android.Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED
import app.squirrel.shared.sync.MediaAccess

private const val TIRAMISU = 33
private const val UPSIDE_DOWN_CAKE = 34

/** Which permissions the app asks for, and what the granted ones mean. Pure: tested on the JVM. */
object MediaPermissions {
    /**
     * Android 13+: images, videos, the "selected photos" grant on 14+ (declared, so partial access is
     * a supported state), media location, notifications (progress). Android 10–12: storage.
     * ACCESS_MEDIA_LOCATION keeps GPS in the bytes (without it the original cannot be read).
     */
    fun toRequest(sdk: Int): List<String> = buildList {
        if (sdk >= TIRAMISU) {
            add(READ_MEDIA_IMAGES)
            add(READ_MEDIA_VIDEO)
            if (sdk >= UPSIDE_DOWN_CAKE) add(READ_MEDIA_VISUAL_USER_SELECTED)
        } else {
            add(READ_EXTERNAL_STORAGE)
        }
        add(ACCESS_MEDIA_LOCATION)
        if (sdk >= TIRAMISU) add(POST_NOTIFICATIONS)
    }

    /**
     * Full when the whole library is readable, NoVideos when only photos are (13+ grants them
     * separately), Partial for "Select photos" (14+, photos and videos alike), else None.
     */
    fun access(sdk: Int, granted: (String) -> Boolean, selectedCount: () -> Int): MediaAccess = when {
        sdk >= TIRAMISU && granted(READ_MEDIA_IMAGES) ->
            if (granted(READ_MEDIA_VIDEO)) MediaAccess.Full else MediaAccess.NoVideos
        sdk >= UPSIDE_DOWN_CAKE && granted(READ_MEDIA_VISUAL_USER_SELECTED) -> MediaAccess.Partial(selectedCount())
        sdk < TIRAMISU && granted(READ_EXTERNAL_STORAGE) -> MediaAccess.Full
        else -> MediaAccess.None
    }

    /** Without ACCESS_MEDIA_LOCATION, MediaStore redacts GPS from the bytes it hands out. */
    fun locationsIncluded(granted: (String) -> Boolean): Boolean = granted(ACCESS_MEDIA_LOCATION)
}
