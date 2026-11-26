package app.squirrel.ui.photos

import kotlin.math.max
import kotlin.math.min

/**
 * A slide across the photos that began with a long press on [anchor]: every photo from [anchor]
 * to the one under the finger, in reading order, is selected, or unselected when [anchor] was
 * selected already. The others keep their state from before the slide ([base]), so sliding back
 * gives the photos left behind their former state.
 */
data class DragSelection(val base: Set<String>, val anchor: String, val adding: Boolean) {
    /** The selection with the finger on [current]; [order] is every photo shown, in reading order. */
    fun over(order: List<String>, current: String): Set<String> {
        val from = order.indexOf(anchor)
        val to = order.indexOf(current)
        if (from < 0 || to < 0) return base
        val range = order.subList(min(from, to), max(from, to) + 1)
        return if (adding) base + range else base - range.toSet()
    }

    companion object {
        fun start(selected: Set<String>, anchor: String) = DragSelection(selected, anchor, adding = anchor !in selected)
    }
}

/** A lazy list item on screen: its key and its vertical span in the list, in pixels. */
data class RowSpan(val key: Any, val top: Int, val bottom: Int)

/**
 * The id of the photo at ([x], [y]), in pixels from the list's top left, among the [rows] on
 * screen; [tilesOf] gives a row's tiles (laid out in dp), null for a row of something else.
 * Null over a heading, a gap or past the last photo.
 */
fun tileAt(rows: List<RowSpan>, x: Float, y: Float, density: Float, tilesOf: (Any) -> List<TileSlot>?): String? {
    val row = rows.firstOrNull { y >= it.top && y < it.bottom } ?: return null
    val dpX = x / density
    return tilesOf(row.key)?.firstOrNull { dpX >= it.left && dpX < it.left + it.width }?.item?.id
}

/**
 * How far to scroll this frame with the finger at [y]: nothing between [top] and [bottom] away
 * from their [zone], then up to [maxStep] (negative: up) the deeper the finger is in an edge zone.
 */
fun edgeScroll(y: Float, top: Float, bottom: Float, zone: Float, maxStep: Float): Float = when {
    y < top + zone -> -maxStep * ((top + zone - y) / zone).coerceAtMost(1f)
    y > bottom - zone -> maxStep * ((y - (bottom - zone)) / zone).coerceAtMost(1f)
    else -> 0f
}
