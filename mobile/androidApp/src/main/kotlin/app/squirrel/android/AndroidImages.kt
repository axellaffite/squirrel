package app.squirrel.android

import android.content.Context
import app.squirrel.ui.images.ImagesPlatform
import coil3.PlatformContext
import okio.Path
import okio.Path.Companion.toOkioPath
import java.io.File

/** Images under the app's private cache directory (not backed up, not visible to other apps). */
class AndroidImages(context: Context) : ImagesPlatform {
    private val dir = File(context.cacheDir, "images")
    override val context: PlatformContext = context.applicationContext
    override val cacheDir: Path = dir.toOkioPath()
    override fun clearFiles() {
        dir.deleteRecursively()
    }
}
