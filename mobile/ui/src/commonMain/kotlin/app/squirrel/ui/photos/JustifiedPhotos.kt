package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import app.squirrel.api.models.AssetSummary
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Tokens

/** [items] in justified rows (no month headings): an album, search results. */
fun justifiedRows(items: List<AssetSummary>, width: Int, rowHeight: Int, gap: Int): List<Pair<Int, List<TileSlot>>> {
    val layout = justify(items.map { a -> (a.width ?: 1).toDouble() / (a.height ?: 1).coerceAtLeast(1) }, width, rowHeight, gap)
    return layout.rows.map { row -> row.height to row.tiles.map { TileSlot(items[it.index], it.left, it.width) } }
}

/**
 * A scrolling list of justified rows; the last row coming into view calls [onNearEnd] (the
 * next page). [header] and [footer] add items above and below the rows (errors, "Try again").
 */
@Composable
fun JustifiedPhotos(
    api: SquirrelApi,
    items: List<AssetSummary>,
    actions: TileActions,
    onNearEnd: () -> Unit,
    header: LazyListScope.() -> Unit = {},
    footer: LazyListScope.() -> Unit = {},
) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val width = maxWidth.value.toInt()
        val rowHeight = Tokens.photoRow.value.toInt()
        val gap = Tokens.space1.value.toInt()
        val rows = remember(items, width) { justifiedRows(items, width, rowHeight, gap) }
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(Tokens.space1),
            contentPadding = PaddingValues(bottom = RailClearance),
        ) {
            header()
            items(rows.size, key = { "row:$it" }) { n ->
                val (height, tiles) = rows[n]
                if (n == rows.lastIndex) LaunchedEffect(items.size) { onNearEnd() }
                PhotoRow(api, height, tiles, actions)
            }
            footer()
        }
    }
}
