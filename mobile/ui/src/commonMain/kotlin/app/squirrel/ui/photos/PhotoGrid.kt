package app.squirrel.ui.photos

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.ThumbnailStatus
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelTheme
import app.squirrel.ui.images.Thumbnail

/** One justified row: tiles at their computed left and width, `--radius-tile`, no shadow. */
@Composable
fun PhotoRow(api: SquirrelApi, height: Int, tiles: List<TileSlot>, onOpen: (AssetSummary) -> Unit) {
    Box(Modifier.fillMaxWidth().height(height.dp)) {
        tiles.forEach { slot -> PhotoTile(api, slot.item, Modifier.offset(x = slot.left.dp).width(slot.width.dp).height(height.dp), onOpen) }
    }
}

@Composable
fun PhotoTile(api: SquirrelApi, item: AssetSummary, modifier: Modifier, onOpen: (AssetSummary) -> Unit) {
    val ready = item.thumbnail.state == ThumbnailStatus.ready
    Thumbnail(
        url = if (ready) api.thumbnailUrl(item.id, item.thumbnail.version) else null,
        blurhash = item.blurhash,
        contentDescription = "Photo taken ${dayLabel(item.takenAt)}",
        unsupported = item.thumbnail.state == ThumbnailStatus.unsupported || item.thumbnail.state == ThumbnailStatus.failed,
        modifier = modifier.clip(Shapes.tile).clickable(role = Role.Image) { onOpen(item) },
    )
}

/** A `--tint` row standing for photos not loaded yet. */
@Composable
fun PlaceholderRow(height: Int) {
    Box(Modifier.fillMaxWidth().height(height.dp).clip(Shapes.tile).background(SquirrelTheme.scheme.tint))
}
