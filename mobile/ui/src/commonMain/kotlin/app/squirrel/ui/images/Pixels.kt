package app.squirrel.ui.images

import androidx.compose.ui.graphics.ImageBitmap

/** An opaque bitmap from ARGB [pixels], written in one pass. */
expect fun imageBitmapOf(pixels: IntArray, width: Int, height: Int): ImageBitmap
