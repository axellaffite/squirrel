package app.squirrel.ui.ios

import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.HandoffResult
import app.squirrel.ui.files.safeFileName
import app.squirrel.ui.files.streamDownload
import app.squirrel.ui.photos.UNREACHABLE
import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.withContext
import platform.CoreGraphics.CGRectMake
import platform.Foundation.NSFileManager
import platform.Foundation.NSFileProtectionCompleteUntilFirstUserAuthentication
import platform.Foundation.NSFileProtectionKey
import platform.Foundation.NSURL
import platform.UIKit.UIActivityViewController
import platform.UIKit.UIDocumentInteractionController
import platform.UIKit.UIViewController

/**
 * Downloads a file's original into `Caches/open/<id>/` (cleared at launch and at sign-out, never
 * while a sheet may still be reading an earlier file), protected until the first unlock after a
 * restart, and hands it over: "Open in" through `UIDocumentInteractionController`, share through
 * `UIActivityViewController`, presented from the Compose host's view controller.
 */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class IosFileOpener(
    private val dir: String = "${cachesPath()}/open",
    /** Where the sheets are presented from; null when the app has no window. */
    private val host: () -> UIViewController? = ::topViewController,
) : FileHandoff {
    private val files = NSFileManager.defaultManager

    override suspend fun open(
        api: SquirrelApi,
        id: String,
        name: String,
        mime: String?,
        share: Boolean,
        onProgress: (Float) -> Unit,
    ): HandoffResult {
        val folder = "$dir/${id.filter { it.isLetterOrDigit() || it == '-' }}"
        val target = "$folder/${safeFileName(name)}"
        val partial = "$target.part"
        try {
            withContext(Dispatchers.IO) {
                files.createDirectoryAtPath(folder, withIntermediateDirectories = true, attributes = PROTECTED, error = null)
                downloadToFile(api, api.contentUrl(id), partial, PROTECTED, onProgress)
                files.removeItemAtPath(target, null) // the same file opened earlier
                if (!files.moveItemAtPath(partial, toPath = target, error = null)) error("could not keep $target")
            }
        } catch (e: CancellationException) {
            files.removeItemAtPath(partial, null)
            throw e
        } catch (e: ApiError) {
            files.removeItemAtPath(partial, null)
            return HandoffResult.Failed(e.message ?: UNREACHABLE)
        } catch (_: Exception) {
            files.removeItemAtPath(partial, null)
            return HandoffResult.Failed(UNREACHABLE)
        }
        val host = host() ?: return HandoffResult.NoApp
        val url = NSURL.fileURLWithPath(target)
        if (share) {
            host.presentViewController(UIActivityViewController(listOf(url), null), animated = true, completion = null)
            return HandoffResult.Opened
        }
        val interaction = UIDocumentInteractionController.interactionControllerWithURL(url)
        current = interaction // kept alive while its menu is up
        val view = host.view
        val shown = interaction.presentOpenInMenuFromRect(CGRectMake(0.0, 0.0, 1.0, 1.0), inView = view, animated = true)
        return if (shown) HandoffResult.Opened else HandoffResult.NoApp
    }

    override fun clear() {
        files.removeItemAtPath(dir, null)
    }

    internal companion object {
        /**
         * Readable once the phone was unlocked after a restart. `Complete` would be stricter, but a
         * share sheet or another app still reading the file breaks when the phone locks.
         */
        val PROTECTED: Map<Any?, Any?> = mapOf(NSFileProtectionKey to NSFileProtectionCompleteUntilFirstUserAuthentication)
        var current: UIDocumentInteractionController? = null
    }
}
