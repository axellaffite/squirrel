package app.squirrel.ui.images

import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asComposeImageBitmap
import org.jetbrains.skia.Bitmap
import org.jetbrains.skia.ColorAlphaType
import org.jetbrains.skia.ColorType
import org.jetbrains.skia.ImageInfo

actual fun imageBitmapOf(pixels: IntArray, width: Int, height: Int): ImageBitmap {
    val bytes = ByteArray(pixels.size * 4)
    pixels.forEachIndexed { i, argb ->
        bytes[i * 4] = argb.toByte() // blue
        bytes[i * 4 + 1] = (argb shr 8).toByte() // green
        bytes[i * 4 + 2] = (argb shr 16).toByte() // red
        bytes[i * 4 + 3] = (argb ushr 24).toByte() // alpha
    }
    val bitmap = Bitmap()
    bitmap.allocPixels(ImageInfo(width, height, ColorType.BGRA_8888, ColorAlphaType.OPAQUE))
    bitmap.installPixels(bytes)
    bitmap.setImmutable()
    return bitmap.asComposeImageBitmap()
}
