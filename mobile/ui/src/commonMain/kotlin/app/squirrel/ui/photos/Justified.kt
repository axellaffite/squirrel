package app.squirrel.ui.photos

import kotlin.math.max

/** A tile placed in its row: [index] into the input, [left] and [width] in the layout's unit. */
data class PlacedTile(val index: Int, val left: Int, val width: Int)

data class JustifiedRow(val top: Int, val height: Int, val tiles: List<PlacedTile>)

data class Justified(val rows: List<JustifiedRow>, val height: Int)

/** Panoramas and slivers are cropped to this range so one photo never owns a row. */
private const val MIN_ASPECT = 0.5
private const val MAX_ASPECT = 3.0

private fun clampAspect(aspect: Double): Double =
    if (!aspect.isFinite() || aspect <= 0) 1.0 else aspect.coerceIn(MIN_ASPECT, MAX_ASPECT)

/** JavaScript's Math.round (half up), so rows match the web pixel for pixel. */
private fun jsRound(value: Double): Int = kotlin.math.floor(value + 0.5).toInt()

/**
 * Greedy justified layout, ported from `web/src/lib/photos/justified.ts`: tiles fill a row until
 * its height, scaled to fill the width exactly, drops to [targetHeight]. The last row keeps
 * [targetHeight] and is left-aligned. Widths are rounded so each full row ends exactly at [width].
 */
fun justify(aspects: List<Double>, width: Int, targetHeight: Int, gap: Int): Justified {
    val rows = mutableListOf<JustifiedRow>()
    var top = 0
    var start = 0
    var aspectSum = 0.0
    fun close(end: Int, height: Double, fill: Boolean) {
        val placed = mutableListOf<PlacedTile>()
        var left = 0
        for (i in start until end) {
            val last = i == end - 1
            val exact = clampAspect(aspects[i]) * height
            val w = if (fill && last) width - left else jsRound(exact)
            placed += PlacedTile(i, left, max(1, w))
            left += w + gap
        }
        val rounded = jsRound(height)
        rows += JustifiedRow(top, rounded, placed)
        top += rounded + gap
    }
    for (i in aspects.indices) {
        aspectSum += clampAspect(aspects[i])
        val count = i - start + 1
        val height = (width - gap * (count - 1)) / aspectSum
        if (height <= targetHeight) {
            close(i + 1, height, fill = true)
            start = i + 1
            aspectSum = 0.0
        }
    }
    if (start < aspects.size) close(aspects.size, targetHeight.toDouble(), fill = false)
    return Justified(rows, max(0, top - gap))
}

/** Rows of square tiles needed for [count] items at [rowHeight]: placeholders before a month loads. */
fun placeholderRows(count: Int, width: Int, rowHeight: Int, gap: Int): Int {
    if (count <= 0) return 0
    val perRow = max(1, (width + gap) / (rowHeight + gap))
    return (count + perRow - 1) / perRow
}

