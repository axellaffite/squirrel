package app.squirrel.shared.ios.photos

import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.addressOf
import kotlinx.cinterop.usePinned
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import platform.Foundation.NSData
import platform.Foundation.NSFileManager
import platform.Foundation.NSTemporaryDirectory
import platform.Foundation.create
import platform.Foundation.writeToFile
import kotlin.test.Test
import kotlin.test.assertContentEquals

@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class ExportReaderTest {
    @Test
    fun chunksReadInParallelGetTheirOwnBytes() = runTest {
        val bytes = ByteArray(1 shl 20) { (it * 31 + it / 4096).toByte() }
        val path = NSTemporaryDirectory() + "export-reader-test"
        bytes.usePinned { NSData.create(bytes = it.addressOf(0), length = bytes.size.toULong()) }.writeToFile(path, atomically = true)
        val reader = AssetExporter(NSTemporaryDirectory() + "export-reader-dir").reader(path, modifiedAt = 1)!!
        val chunk = 64 * 1024
        withContext(Dispatchers.Default) {
            (0 until bytes.size / chunk).map { index ->
                async {
                    val into = ByteArray(chunk)
                    var filled = 0
                    while (filled < chunk) filled += reader.read(index.toLong() * chunk + filled, into, filled, chunk - filled)
                    index to into
                }
            }.awaitAll()
        }.forEach { (index, got) ->
            assertContentEquals(bytes.copyOfRange(index * chunk, (index + 1) * chunk), got, "chunk $index")
        }
        reader.close()
        NSFileManager.defaultManager.removeItemAtPath(path, null)
    }
}
