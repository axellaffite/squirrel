package app.squirrel.shared.ios.photos

import app.squirrel.shared.hash.Sha256
import app.squirrel.shared.sync.AssetReader
import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.addressOf
import kotlinx.cinterop.usePinned
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import platform.Foundation.NSCachesDirectory
import platform.Foundation.NSDate
import platform.Foundation.NSFileHandle
import platform.Foundation.NSFileManager
import platform.Foundation.NSFileModificationDate
import platform.Foundation.NSFileSize
import platform.Foundation.NSNumber
import platform.Foundation.NSSearchPathForDirectoriesInDomains
import platform.Foundation.NSURL
import platform.Foundation.NSUserDomainMask
import platform.Foundation.closeFile
import platform.Foundation.fileHandleForReadingAtPath
import platform.Foundation.readDataOfLength
import platform.Foundation.seekToFileOffset
import platform.Foundation.timeIntervalSince1970
import platform.Photos.PHAssetResource
import platform.Photos.PHAssetResourceManager
import platform.Photos.PHAssetResourceRequestOptions
import platform.posix.memcpy
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** Exports beyond this are swept, oldest first. */
const val EXPORT_CAP_BYTES = 2L * 1024 * 1024 * 1024

/**
 * Copies an asset's original out of the photo library into `Caches/export/` (the bytes PhotoKit
 * hands out are not a file the app can seek in otherwise), and reads it back by range.
 */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class AssetExporter(private val dir: String = defaultDir()) {
    private val files = NSFileManager.defaultManager

    /** The export of [resource] for this version of the asset ([version] changes on edit). */
    suspend fun export(id: String, version: Long, resource: PHAssetResource, allowNetwork: Boolean): String {
        files.createDirectoryAtPath(dir, withIntermediateDirectories = true, attributes = null, error = null)
        val path = "$dir/${key(id, version)}"
        if (files.fileExistsAtPath(path)) return path
        val partial = "$path.part"
        files.removeItemAtPath(partial, null)
        val options = PHAssetResourceRequestOptions().apply { networkAccessAllowed = allowNetwork }
        suspendCancellableCoroutine { continuation ->
            PHAssetResourceManager.defaultManager().writeDataForAssetResource(
                resource,
                toFile = NSURL.fileURLWithPath(partial),
                options = options,
            ) { error ->
                if (error == null) continuation.resume(Unit)
                else continuation.resumeWithException(ExportFailed(error.localizedDescription))
            }
        }
        files.moveItemAtPath(partial, toPath = path, error = null)
        return path
    }

    /** Removes every export of [id] (after its upload completed). */
    fun forget(id: String) {
        val prefix = keyPrefix(id)
        files.contentsOfDirectoryAtPath(dir, null)?.forEach { name ->
            if ((name as String).startsWith(prefix)) files.removeItemAtPath("$dir/$name", null)
        }
    }

    /** Keeps the exports under [cap] bytes, deleting the least recently written first. */
    fun sweep(cap: Long = EXPORT_CAP_BYTES) {
        val entries = files.contentsOfDirectoryAtPath(dir, null).orEmpty().map { "$dir/$it" }.mapNotNull { path ->
            val attributes = files.attributesOfItemAtPath(path, null) ?: return@mapNotNull null
            val size = (attributes[NSFileSize] as? NSNumber)?.longLongValue ?: 0L
            val modified = (attributes[NSFileModificationDate] as? NSDate)?.timeIntervalSince1970 ?: 0.0
            Triple(path, size, modified)
        }.sortedByDescending { it.third }
        var total = 0L
        entries.forEach { (path, size, _) ->
            total += size
            if (total > cap) files.removeItemAtPath(path, null)
        }
    }

    fun reader(path: String, modifiedAt: Long): AssetReader? {
        val size = (files.attributesOfItemAtPath(path, null)?.get(NSFileSize) as? NSNumber)?.longLongValue ?: return null
        val handle = NSFileHandle.fileHandleForReadingAtPath(path) ?: return null
        return FileReader(handle, size, modifiedAt)
    }

    private class FileReader(private val handle: NSFileHandle, override val size: Long, override val modifiedAt: Long) : AssetReader {
        override suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int = withContext(Dispatchers.IO) {
            if (position >= size) return@withContext -1
            handle.seekToFileOffset(position.toULong())
            val data = handle.readDataOfLength(length.toULong())
            val count = data.length.toInt()
            if (count == 0) return@withContext -1
            into.usePinned { memcpy(it.addressOf(offset), data.bytes, count.toULong()) }
            count
        }

        override fun close() = handle.closeFile()
    }

    private fun keyPrefix(id: String): String = Sha256().apply { id.encodeToByteArray().let { update(it, 0, it.size) } }.digestHex().take(32)

    private fun key(id: String, version: Long) = "${keyPrefix(id)}-$version"

    companion object {
        fun defaultDir(): String =
            (NSSearchPathForDirectoriesInDomains(NSCachesDirectory, NSUserDomainMask, true).first() as String) + "/export"
    }
}

class ExportFailed(message: String) : Exception(message)
