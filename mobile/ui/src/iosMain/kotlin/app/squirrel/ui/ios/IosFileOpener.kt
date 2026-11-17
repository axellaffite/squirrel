package app.squirrel.ui.ios

import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.throwUnlessSuccess
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.HandoffResult
import app.squirrel.ui.files.safeFileName
import app.squirrel.ui.photos.UNREACHABLE
import io.ktor.client.plugins.HttpTimeoutConfig
import io.ktor.client.plugins.timeout
import io.ktor.client.request.prepareGet
import io.ktor.client.statement.bodyAsChannel
import io.ktor.http.contentLength
import io.ktor.utils.io.readAvailable
import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.addressOf
import kotlinx.cinterop.usePinned
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.withContext
import platform.CoreGraphics.CGRectMake
import platform.Foundation.NSData
import platform.Foundation.NSFileHandle
import platform.Foundation.NSFileManager
import platform.Foundation.NSURL
import platform.Foundation.closeFile
import platform.Foundation.create
import platform.Foundation.fileHandleForWritingAtPath
import platform.Foundation.writeData
import platform.UIKit.UIActivityViewController
import platform.UIKit.UIDocumentInteractionController
import platform.UIKit.UIViewController

/**
 * Downloads a file's original into `Caches/open/<id>/` (cleared on the next open and at sign-out)
 * and hands it over: "Open in" through `UIDocumentInteractionController`, share through
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
                clear()
                files.createDirectoryAtPath(folder, withIntermediateDirectories = true, attributes = null, error = null)
                download(api, id, partial, onProgress)
                files.moveItemAtPath(partial, toPath = target, error = null)
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

    private suspend fun download(api: SquirrelApi, id: String, path: String, onProgress: (Float) -> Unit) {
        files.createFileAtPath(path, contents = null, attributes = null)
        val out = NSFileHandle.fileHandleForWritingAtPath(path) ?: error("cannot write $path")
        try {
            api.httpClient.prepareGet(api.contentUrl(id)) {
                timeout { requestTimeoutMillis = HttpTimeoutConfig.INFINITE_TIMEOUT_MS } // large originals
            }.execute { response ->
                response.throwUnlessSuccess()
                val total = response.contentLength()
                val channel = response.bodyAsChannel()
                val buffer = ByteArray(BUFFER)
                var done = 0L
                while (true) {
                    val n = channel.readAvailable(buffer, 0, buffer.size)
                    if (n < 0) break
                    if (n == 0) {
                        if (channel.isClosedForRead) break
                        continue
                    }
                    out.writeData(buffer.usePinned { NSData.create(bytes = it.addressOf(0), length = n.toULong()) })
                    done += n
                    if (total != null && total > 0) onProgress((done.toFloat() / total).coerceAtMost(1f))
                }
            }
        } finally {
            out.closeFile()
        }
    }

    override fun clear() {
        files.removeItemAtPath(dir, null)
    }

    private companion object {
        const val BUFFER = 64 * 1024
        var current: UIDocumentInteractionController? = null
    }
}
