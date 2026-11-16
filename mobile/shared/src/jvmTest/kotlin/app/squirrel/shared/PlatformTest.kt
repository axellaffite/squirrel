package app.squirrel.shared

import kotlin.test.Test
import kotlin.test.assertEquals

class PlatformTest {
    @Test
    fun jvmPlatformIsNamed() {
        assertEquals("JVM", platformName())
    }
}
