package app.squirrel.android.files

import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue

class OpenCacheTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val clock = System.currentTimeMillis()
    private val root by lazy { File(folder.root, "open") }
    private val cache by lazy { OpenCache(root, now = { clock }) }

    private fun write(file: File, at: Long): File {
        file.parentFile.mkdirs()
        file.writeText("bytes")
        file.setLastModified(at)
        return file
    }

    @Test
    fun pruningKeepsTheOpenedFileAndRecentOnes() {
        val old = write(cache.target("old", "a.pdf"), at = clock - 2 * OpenCache.KEEP_MS)
        val recent = write(cache.target("recent", "b.pdf"), at = clock - 60_000)
        val opening = write(cache.target("opening", "c.pdf"), at = clock - 3 * OpenCache.KEEP_MS)

        cache.prune(keepId = "opening")

        assertFalse(old.exists())
        assertTrue(recent.exists())
        assertTrue(opening.exists())
    }

    @Test
    fun twoDownloadsOfOneFileUseTheirOwnTempFiles() {
        val target = cache.target("id-1", "Report.pdf")
        val a = cache.temp(target)
        val b = cache.temp(target)
        assertNotEquals(a, b)
        a.writeText("first")
        b.writeText("second")

        cache.commit(b, target)
        cache.commit(a, target)

        assertEquals("first", target.readText())
        assertFalse(a.exists() || b.exists())
    }

    @Test
    fun namesNeverBecomePaths() {
        assertEquals("_.._etc_passwd", OpenCache.safeName("/../etc/passwd"))
        assertEquals("file", OpenCache.safeName("   "))
        assertEquals(File(File(root, "x1"), "a.jpg"), cache.target("x/1", "a.jpg"))
    }
}
