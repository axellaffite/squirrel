package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import app.squirrel.api.models.AssetSummary
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Tokens

/**
 * Photos already in capture order (newest first) as months, each holding all of its loaded
 * photos: nothing waits on a month of its own, so the timeline has no placeholder rows.
 */
fun datedMonths(items: List<AssetSummary>): List<MonthPage> =
    items.groupBy { it.takenAt.take(7) }.map { (month, photos) ->
        MonthPage(month, photos.size.toLong(), photos, nextCursor = null, started = true)
    }

/**
 * [items] (newest capture first, paged by [onNearEnd]) as a timeline: month headings over
 * justified rows, as the Photos tab shows them.
 */
@Composable
fun DatedPhotos(
    api: SquirrelApi,
    items: List<AssetSummary>,
    actions: TileActions,
    onNearEnd: () -> Unit,
    footer: LazyListScope.() -> Unit = {},
) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val width = maxWidth.value.toInt()
        val layout = remember { TimelineLayout() }
        val blocks = remember(items, width) { layout.blocks(datedMonths(items), width) }
        val rows = remember(blocks) { blocks.filterIsInstance<TimelineBlock.Row>().associate { it.key to it.tiles } }
        val last = blocks.lastOrNull()?.key
        val list = rememberLazyListState()
        val select = rememberDragToSelect(list, actions, order = { items.map { it.id } }, tilesOf = { rows[it] })
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize().then(select),
            verticalArrangement = Arrangement.spacedBy(Tokens.space1),
            contentPadding = PaddingValues(bottom = RailClearance),
        ) {
            items(blocks, key = { it.key }, contentType = { it::class }) { block ->
                if (block.key == last) LaunchedEffect(items.size) { onNearEnd() }
                when (block) {
                    is TimelineBlock.Header -> MonthHeading(block.label)
                    is TimelineBlock.Row -> PhotoRow(api, block.height, block.tiles, actions)
                    is TimelineBlock.Placeholder -> PlaceholderRow(block.height)
                }
            }
            footer()
        }
    }
}
