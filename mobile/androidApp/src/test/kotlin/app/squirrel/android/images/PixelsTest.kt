package app.squirrel.android.images

import app.squirrel.ui.images.imageBitmapOf
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PixelsTest {
    @Test
    fun theBitmapHoldsThePixelsAsGiven() {
        val pixels = intArrayOf(0xFFFF0000.toInt(), 0xFF00FF00.toInt(), 0xFF0000FF.toInt(), 0xFF979695.toInt(), 0xFF000000.toInt(), 0xFFFFFFFF.toInt())
        val bitmap = imageBitmapOf(pixels, 3, 2)
        assertEquals(3, bitmap.width)
        assertEquals(2, bitmap.height)
        val back = IntArray(6)
        bitmap.readPixels(back)
        assertEquals(pixels.toList(), back.toList())
    }
}
