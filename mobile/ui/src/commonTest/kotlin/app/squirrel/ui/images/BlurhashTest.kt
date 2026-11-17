package app.squirrel.ui.images

import kotlin.math.roundToInt
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/** The web's vectors (web/src/lib/photos/blurhash.test.ts). */
class BlurhashTest {
    private val hash = "LEHV6nWB2yk8pyo0adR*.7kCMdnj"

    @Test
    fun validatesLengthAndAlphabet() {
        assertTrue(Blurhash.isValid(hash))
        assertFalse(Blurhash.isValid("LEHV6nWB2yk8"))
        assertFalse(Blurhash.isValid("L!!!!!!!!!!!!!!!!!!!!!!!!!!"))
        assertFalse(Blurhash.isValid(null))
    }

    @Test
    fun theAverageColourNeedsNoDecoding() {
        assertEquals(0xFF979695.toInt(), Blurhash.averageColor(hash)) // rgb(151 150 149)
    }

    @Test
    fun decodesOpaquePixelsWhoseMeanIsTheAverage() {
        val px = Blurhash.decode(hash, 32, 32)
        assertEquals(32 * 32, px.size)
        val sum = IntArray(3)
        px.forEach { argb ->
            sum[0] += (argb shr 16) and 0xFF
            sum[1] += (argb shr 8) and 0xFF
            sum[2] += argb and 0xFF
        }
        assertEquals(listOf(150, 150, 148), sum.map { (it / 1024.0).roundToInt() })
        assertEquals(0xFF, (px[0] ushr 24) and 0xFF)
    }
}
