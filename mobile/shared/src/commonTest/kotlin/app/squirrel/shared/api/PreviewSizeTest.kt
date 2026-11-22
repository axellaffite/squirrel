package app.squirrel.shared.api

import kotlin.test.Test
import kotlin.test.assertEquals

class PreviewSizeTest {
    private data class Vector(val photo: Pair<Int, Int>?, val viewport: Pair<Int, Int>, val density: Double, val px: Int)

    /** Shared with the web (`web/src/lib/photos/previewSize.test.ts`): keep both tables identical. */
    private val vectors = listOf(
        // A 12 MP landscape photo on a 1080p screen, a 1366 × 768 one at 2×, then a 4K one.
        Vector(4000 to 3000, 1920 to 1080, 1.0, 1440),
        Vector(4000 to 3000, 1366 to 768, 2.0, 2160),
        Vector(4000 to 3000, 3840 to 2160, 1.0, 3200),
        // A portrait photo on a phone (393 × 852 points at 3×).
        Vector(3024 to 4032, 393 to 852, 3.0, 2160),
        // A landscape photo on the same phone: limited by the width.
        Vector(4032 to 3024, 393 to 852, 3.0, 1440),
        // Never above the photo itself.
        Vector(1200 to 800, 3840 to 2160, 2.0, 1440),
        Vector(2000 to 1500, 3840 to 2160, 2.0, 2160),
        // Size unknown: the screen's long side.
        Vector(null, 1280 to 800, 2.0, 3200),
        Vector(null, 800 to 600, 1.0, 1440),
        // Nonsense ratios count as 1.
        Vector(4000 to 3000, 1920 to 1080, 0.0, 1440),
    )

    @Test
    fun theSharedVectorsGiveTheSameBuckets() {
        for (v in vectors) {
            val px = PreviewSize.px(v.photo?.first, v.photo?.second, v.viewport.first.toDouble(), v.viewport.second.toDouble(), v.density)
            assertEquals(v.px, px, v.toString())
        }
    }

    @Test
    fun pxIsRoundedUpToABucketAndCapped() {
        assertEquals(1440, PreviewSize.bucketFor(1.0))
        assertEquals(1440, PreviewSize.bucketFor(1440.0))
        assertEquals(2160, PreviewSize.bucketFor(1441.0))
        assertEquals(3200, PreviewSize.bucketFor(10_000.0))
    }
}
