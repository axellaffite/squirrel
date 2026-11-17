package app.squirrel.ui.files

import kotlin.test.Test
import kotlin.test.assertEquals

class SafeFileNameTest {
    @Test
    fun aNameNeverBecomesAPath() {
        assertEquals("Tax return 2025.pdf", safeFileName("Tax return 2025.pdf"))
        assertEquals("_etc_passwd", safeFileName("/etc/passwd"))
        assertEquals("_..__secret", safeFileName("/../_secret"))
        assertEquals("a_b", safeFileName("a\\b"))
        assertEquals("hidden", safeFileName("..hidden"))
        assertEquals("file", safeFileName("  "))
        assertEquals("file", safeFileName("..."))
    }
}
