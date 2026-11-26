package app.squirrel.ui.photos

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.ThumbnailStatus
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.boxShadow
import app.squirrel.theme.components.Icon
import app.squirrel.ui.images.Thumbnail

/**
 * How tiles answer touches: a tap opens, or toggles while selecting; a long press starts
 * selecting, and sliding on from it selects a range (see [rememberDragToSelect]).
 */
class TileActions(
    val onOpen: (AssetSummary) -> Unit,
    val selected: Set<String> = emptySet(),
    /** The new selection; null: this grid has no selection. */
    val onSelect: ((Set<String>) -> Unit)? = null,
) {
    val selecting: Boolean get() = selected.isNotEmpty()

    fun toggle(item: AssetSummary) {
        onSelect?.invoke(selected.toggled(item.id))
    }

    companion object {
        fun open(onOpen: (AssetSummary) -> Unit) = TileActions(onOpen)
    }
}

/** One justified row: tiles at their computed left and width, `--radius-tile`, no shadow. */
@Composable
fun PhotoRow(api: SquirrelApi, height: Int, tiles: List<TileSlot>, actions: TileActions) {
    Box(Modifier.fillMaxWidth().height(height.dp)) {
        tiles.forEach { slot -> PhotoTile(api, slot.item, Modifier.offset(x = slot.left.dp).width(slot.width.dp).height(height.dp), actions) }
    }
}

/**
 * MASTER.md photo tile. Selected: `--select-ring` and a filled `--accent` check circle with an
 * `--on-accent` check, top left.
 */
@Composable
fun PhotoTile(api: SquirrelApi, item: AssetSummary, modifier: Modifier, actions: TileActions) {
    val ready = item.thumbnail.state == ThumbnailStatus.ready
    val selected = item.id in actions.selected
    val selectable = actions.onSelect != null
    val scheme = SquirrelTheme.scheme
    Box(
        modifier
            .then(if (selected) Modifier.boxShadow(scheme.selectRing, Shapes.tile) else Modifier)
            .clip(Shapes.tile)
            .semantics {
                if (selectable) {
                    this.selected = selected
                    // The list's long press (and slide) is a pointer gesture: assistive tech gets it here.
                    onLongClick("Select") { actions.toggle(item); true }
                }
            }
            .clickable(
                role = Role.Button,
                onClickLabel = if (actions.selecting && selectable) "Select" else "Open",
                onClick = { if (actions.selecting && selectable) actions.toggle(item) else actions.onOpen(item) },
            ),
    ) {
        Thumbnail(
            url = if (ready) api.thumbnailUrl(item.id, item.thumbnail.version) else null,
            blurhash = item.blurhash,
            contentDescription = photoLabel(item),
            unsupported = item.thumbnail.state == ThumbnailStatus.unsupported || item.thumbnail.state == ThumbnailStatus.failed,
            modifier = Modifier.matchParentSize(),
        )
        if (selected) {
            Box(
                Modifier.align(Alignment.TopStart).padding(Tokens.space1).size(Tokens.selectCheck).background(scheme.accent, Shapes.circle),
                contentAlignment = Alignment.Center,
            ) {
                Icon(SquirrelIcon.Check, contentDescription = null, tint = scheme.onAccent, size = Tokens.iconSm)
            }
        }
    }
}

/** A `--tint` row standing for photos not loaded yet. */
@Composable
fun PlaceholderRow(height: Int) {
    Box(Modifier.fillMaxWidth().height(height.dp).clip(Shapes.tile).background(SquirrelTheme.scheme.tint))
}
