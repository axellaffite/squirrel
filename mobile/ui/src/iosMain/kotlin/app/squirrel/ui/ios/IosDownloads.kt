package app.squirrel.ui.ios

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.files.streamDownload
import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.addressOf
import kotlinx.cinterop.usePinned
import platform.Foundation.NSData
import platform.Foundation.NSFileHandle
import platform.Foundation.NSFileManager
import platform.Foundation.closeFile
import platform.Foundation.create
import platform.Foundation.fileHandleForWritingAtPath
import platform.Foundation.writeData

/** Streams `GET url` into a new file at [path] created with [attributes] (file protection). */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
internal suspend fun downloadToFile(api: SquirrelApi, url: String, path: String, attributes: Map<Any?, Any?>, onProgress: (Float) -> Unit) {
    if (!NSFileManager.defaultManager.createFileAtPath(path, contents = null, attributes = attributes)) error("cannot create $path")
    val out = NSFileHandle.fileHandleForWritingAtPath(path) ?: error("cannot write $path")
    try {
        streamDownload(api, url, onProgress) { buffer, n ->
            out.writeData(buffer.usePinned { NSData.create(bytes = it.addressOf(0), length = n.toULong()) })
        }
    } finally {
        out.closeFile()
    }
}
