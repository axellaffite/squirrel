package app.squirrel.ui.ios

import app.squirrel.ui.images.ImagesPlatform
import coil3.PlatformContext
import kotlinx.cinterop.ExperimentalForeignApi
import okio.Path
import okio.Path.Companion.toPath
import platform.Foundation.NSCachesDirectory
import platform.Foundation.NSFileManager
import platform.Foundation.NSSearchPathForDirectoriesInDomains
import platform.Foundation.NSUserDomainMask

/** The app's Caches directory (never backed up, purgeable by the system). */
fun cachesPath(): String =
    NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, true).first() as String

/** Images under Caches/images. */
class IosImages : ImagesPlatform {
    private val dir = "${cachesPath()}/images"
    override val context: PlatformContext = PlatformContext.INSTANCE
    override val cacheDir: Path = dir.toPath()

    @OptIn(ExperimentalForeignApi::class)
    override fun clearFiles() {
        NSFileManager.defaultManager.removeItemAtPath(dir, null)
    }
}
