package app.squirrel.shared.api

import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/**
 * The `px` the photo viewer asks for: the photo fitted into the viewport, in device pixels, its
 * long side rounded up to one of the server's buckets (a few URLs cover every screen, and each
 * stays cached). The web computes the same (`web/src/lib/photos/previewSize.ts`, same test vectors).
 */
object PreviewSize {
    /** Long sides the server renders previews at (`squirrel_media::preview::BUCKETS`). */
    val BUCKETS = listOf(1440, 2160, 3200)

    /** The smallest bucket at least [px] long, or the largest. */
    fun bucketFor(px: Double): Int = BUCKETS.firstOrNull { it >= px } ?: BUCKETS.last()

    /**
     * A photo of [photoWidth] × [photoHeight] as displayed (null or 0 when unknown) contained in
     * a [viewportWidth] × [viewportHeight] viewport of logical units at [density] device pixels
     * each: the bucket of its long side on screen, never above the photo's own long side.
     */
    fun px(photoWidth: Int?, photoHeight: Int?, viewportWidth: Double, viewportHeight: Double, density: Double): Int {
        val ratio = if (density.isFinite() && density > 0) density else 1.0
        val screenWidth = max(1.0, viewportWidth) * ratio
        val screenHeight = max(1.0, viewportHeight) * ratio
        if (photoWidth == null || photoHeight == null || photoWidth <= 0 || photoHeight <= 0) {
            return bucketFor(max(screenWidth, screenHeight))
        }
        val scale = min(screenWidth / photoWidth, screenHeight / photoHeight)
        val long = max(photoWidth, photoHeight).toDouble()
        return bucketFor(min(ceil(long * scale), long))
    }
}
