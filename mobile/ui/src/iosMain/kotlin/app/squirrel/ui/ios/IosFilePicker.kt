package app.squirrel.ui.ios

import app.squirrel.shared.ios.photos.fileAssetReader
import app.squirrel.ui.files.FilePicker
import app.squirrel.ui.files.PickedFile
import app.squirrel.ui.files.safeFileName
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.coroutines.suspendCancellableCoroutine
import platform.Foundation.NSFileManager
import platform.Foundation.NSURL
import platform.UIKit.UIDocumentPickerDelegateProtocol
import platform.UIKit.UIDocumentPickerViewController
import platform.UniformTypeIdentifiers.UTTypeItem
import platform.darwin.NSObject
import kotlin.coroutines.resume

/**
 * The system's document picker (Files, iCloud Drive, other apps' providers). Picked files are
 * copied into the app's temporary folder by the system (`asCopy`), read from there, and deleted
 * once uploaded.
 */
class IosFilePicker(private val host: () -> platform.UIKit.UIViewController? = ::topViewController) : FilePicker {
    /** The delegate of the sheet on screen: UIKit holds it weakly. */
    private var delegate: PickerDelegate? = null

    override suspend fun pick(): List<PickedFile> = suspendCancellableCoroutine { continuation ->
        val presenter = host()
        if (presenter == null) {
            continuation.resume(emptyList())
            return@suspendCancellableCoroutine
        }
        val controller = UIDocumentPickerViewController(forOpeningContentTypes = listOf(UTTypeItem), asCopy = true)
        controller.allowsMultipleSelection = true
        val picked = PickerDelegate { urls ->
            delegate = null
            if (continuation.isActive) continuation.resume(urls.map(::pickedFile))
        }
        delegate = picked
        controller.delegate = picked
        presenter.presentViewController(controller, animated = true, completion = null)
    }
}

@OptIn(ExperimentalForeignApi::class)
private fun pickedFile(url: NSURL): PickedFile {
    val path = url.path.orEmpty()
    return PickedFile(
        name = safeFileName(url.lastPathComponent ?: "file"),
        open = { fileAssetReader(path) },
        release = { NSFileManager.defaultManager.removeItemAtURL(url, null) },
    )
}

private class PickerDelegate(private val done: (List<NSURL>) -> Unit) : NSObject(), UIDocumentPickerDelegateProtocol {
    override fun documentPicker(controller: UIDocumentPickerViewController, didPickDocumentsAtURLs: List<*>) {
        done(didPickDocumentsAtURLs.filterIsInstance<NSURL>())
    }

    override fun documentPickerWasCancelled(controller: UIDocumentPickerViewController) {
        done(emptyList())
    }
}
