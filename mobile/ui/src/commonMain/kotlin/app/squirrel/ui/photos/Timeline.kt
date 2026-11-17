package app.squirrel.ui.photos

import androidx.compose.ui.geometry.Offset

import app.squirrel.api.models.AssetSummary
import app.squirrel.theme.Tokens
import kotlinx.datetime.Month

/** One month of the timeline: its count from `/api/photos/months` and the items loaded so far. */
data class MonthPage(
    val month: String,
    val count: Long,
    val items: List<AssetSummary> = emptyList(),
    val nextCursor: String? = null,
    val started: Boolean = false,
    val loading: Boolean = false,
    val failed: Boolean = false,
) {
    val complete: Boolean get() = started && nextCursor == null
    val label: String get() = monthLabel(month)
}

data class PhotosState(
    val months: List<MonthPage> = emptyList(),
    val loadingMonths: Boolean = true,
    val error: String? = null,
) {
    val busy: Boolean get() = loadingMonths || months.any { it.loading }
    val empty: Boolean get() = !loadingMonths && error == null && months.isEmpty()

    /** Every loaded photo, newest first: what the viewer pages through. */
    fun loaded(): List<AssetSummary> = months.flatMap { it.items }

    fun indexOf(id: String): Int = loaded().indexOfFirst { it.id == id }
}

/** "2024-07" → "July 2024". */
fun monthLabel(month: String): String {
    val year = month.substringBefore('-')
    val number = month.substringAfter('-').toIntOrNull() ?: return month
    val name = Month.entries.getOrNull(number - 1)?.name ?: return month
    return name.lowercase().replaceFirstChar { it.uppercase() } + " " + year
}

/** "2024-07-14T18:30:05" → "14 July 2024" (the photo's own wall clock). */
fun dayLabel(takenAt: String): String {
    val date = takenAt.take(10)
    val day = date.substringAfterLast('-').toIntOrNull() ?: return takenAt
    return "$day ${monthLabel(date.take(7))}"
}

/** What assistive tech reads for a photo: "Photo, 12 October 2026, 19:59". */
fun photoLabel(item: AssetSummary): String {
    val kind = if (item.mime?.startsWith("video/") == true) "Video" else "Photo"
    val time = item.takenAt.drop(11).take(5)
    return listOf(kind, dayLabel(item.takenAt), time).filter { it.isNotEmpty() }.joinToString(", ")
}

/** Zoom pan bounds: the scaled photo's edges never come inside the stage. */
fun clampPan(offset: Offset, scale: Float, width: Float, height: Float): Offset {
    val maxX = (width * (scale - 1) / 2).coerceAtLeast(0f)
    val maxY = (height * (scale - 1) / 2).coerceAtLeast(0f)
    return Offset(offset.x.coerceIn(-maxX, maxX), offset.y.coerceIn(-maxY, maxY))
}

data class TileSlot(val item: AssetSummary, val left: Int, val width: Int)

/** What the timeline's LazyColumn shows, in order. */
sealed interface TimelineBlock {
    val key: String
    val month: String

    data class Header(override val month: String, val label: String) : TimelineBlock {
        override val key get() = "h:$month"
    }

    data class Row(override val month: String, val index: Int, val height: Int, val tiles: List<TileSlot>) : TimelineBlock {
        override val key get() = "r:$month:$index"
    }

    /** A `--tint` row standing for photos not loaded yet (never fake photos). */
    data class Placeholder(override val month: String, val index: Int, val height: Int) : TimelineBlock {
        override val key get() = "p:$month:$index"
    }
}

private fun AssetSummary.aspect(): Double {
    val w = width ?: return 1.0
    val h = height ?: return 1.0
    return if (h == 0) 1.0 else w.toDouble() / h
}

/** Header, justified rows of what is loaded, then placeholder rows for the rest, per month. */
fun timelineBlocks(
    months: List<MonthPage>,
    width: Int,
    rowHeight: Int = Tokens.photoRow.value.toInt(),
    gap: Int = Tokens.space1.value.toInt(),
): List<TimelineBlock> = buildList {
    for (page in months) {
        add(TimelineBlock.Header(page.month, page.label))
        val layout = justify(page.items.map { it.aspect() }, width, rowHeight, gap)
        layout.rows.forEachIndexed { n, row ->
            add(TimelineBlock.Row(page.month, n, row.height, row.tiles.map { TileSlot(page.items[it.index], it.left, it.width) }))
        }
        val remaining = (page.count - page.items.size).coerceAtLeast(0).toInt()
        if (!page.complete) {
            repeat(placeholderRows(remaining, width, rowHeight, gap)) { add(TimelineBlock.Placeholder(page.month, it, rowHeight)) }
        }
    }
}
