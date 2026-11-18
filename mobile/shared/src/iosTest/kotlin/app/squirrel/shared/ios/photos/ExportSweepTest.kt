package app.squirrel.shared.ios.photos

import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.addressOf
import kotlinx.cinterop.usePinned
import platform.Foundation.NSData
import platform.Foundation.NSFileManager
import platform.Foundation.NSTemporaryDirectory
import platform.Foundation.NSUUID
import platform.Foundation.create
import platform.Foundation.writeToFile
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class ExportSweepTest {
    private val dir = "${NSTemporaryDirectory()}sweep-${NSUUID().UUIDString}"
    private val files = NSFileManager.defaultManager
    private val exporter = AssetExporter(dir)

    @AfterTest
    fun cleanUp() {
        files.removeItemAtPath(dir, null)
    }

    private fun export(id: String): String {
        files.createDirectoryAtPath(dir, withIntermediateDirectories = true, attributes = null, error = null)
        val bytes = ByteArray(1000)
        val path = exporter.pathOf(id, 1)
        bytes.usePinned { NSData.create(bytes = it.addressOf(0), length = bytes.size.toULong()) }.writeToFile(path, atomically = true)
        return path
    }

    @Test
    fun anUploadInProgressKeepsItsExport() {
        val uploading = export("video:icloud")
        val done = export("photo:old")

        exporter.sweep(cap = 100, keep = setOf("video:icloud"))

        assertTrue(files.fileExistsAtPath(uploading))
        assertFalse(files.fileExistsAtPath(done))
    }

    @Test
    fun withNothingToKeepTheCapApplies() {
        val path = export("video:icloud")
        exporter.sweep(cap = 100)
        assertFalse(files.fileExistsAtPath(path))
    }
}
