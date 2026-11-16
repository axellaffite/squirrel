package app.squirrel.android.media

import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.provider.MediaStore
import androidx.core.content.ContextCompat
import app.squirrel.shared.sync.MediaAccess

/** Reads the current photo access; re-read before each run and on resume (partial access can shrink). */
class MediaAccessReader(private val context: Context) {
    private fun granted(permission: String) =
        ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

    fun current(): MediaAccess = MediaPermissions.access(Build.VERSION.SDK_INT, ::granted, ::visibleCount)

    fun locationsIncluded(): Boolean = MediaPermissions.locationsIncluded(::granted)

    /** With partial access MediaStore only returns the selected items: count them. */
    private fun visibleCount(): Int = listOf(
        MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL),
        MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL),
    ).sumOf { uri ->
        context.contentResolver.query(uri, arrayOf(MediaStore.MediaColumns._ID), null, null, null)?.use { it.count } ?: 0
    }
}
