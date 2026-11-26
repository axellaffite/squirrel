package app.squirrel.ui

import androidx.compose.ui.window.ComposeUIViewController
import app.squirrel.shared.ios.photos.fileAssetReader
import app.squirrel.ui.files.PickedFile
import app.squirrel.ui.files.safeFileName
import app.squirrel.ui.ios.IosGraph
import app.squirrel.ui.send.SendApp
import kotlinx.cinterop.ExperimentalForeignApi
import platform.Foundation.NSFileManager
import platform.UIKit.UIViewController

/**
 * The Share extension's screen: [paths] are the files it copied out of the share (deleted once
 * sent), [onClose] completes the extension's request.
 */
@OptIn(ExperimentalForeignApi::class)
@Suppress("FunctionName")
fun ShareViewController(paths: List<String>, onClose: () -> Unit): UIViewController {
    val files = paths.map { path ->
        PickedFile(
            name = safeFileName(path.substringAfterLast('/')),
            open = { fileAssetReader(path) },
            release = { NSFileManager.defaultManager.removeItemAtPath(path, null) },
        )
    }
    return ComposeUIViewController { SendApp(IosGraph.instance, files, onClose) }
}
