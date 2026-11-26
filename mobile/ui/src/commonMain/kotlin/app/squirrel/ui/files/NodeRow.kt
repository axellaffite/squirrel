package app.squirrel.ui.files

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.NodeKind
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.sync.formatBytes
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.Icon
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.Progress
import app.squirrel.theme.components.RowPlace
import app.squirrel.theme.components.RowTile
import app.squirrel.ui.images.Thumbnail

/** Pills from [rootLabel] ("Files", or "Shared" for someone else's folder) to the current folder. */
@Composable
fun Breadcrumbs(rootLabel: String, path: List<Pair<String, String>>, onRoot: () -> Unit, onGo: (String) -> Unit) {
    Row(
        modifier = Modifier.horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space1),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Button(rootLabel, onClick = onRoot, variant = ButtonVariant.Quiet, size = ButtonSize.Sm)
        path.forEachIndexed { index, (id, name) ->
            Icon(SquirrelIcon.ChevronRight, contentDescription = null, tint = SquirrelTheme.scheme.inkMuted)
            val last = index == path.lastIndex
            Button(name, onClick = { if (!last) onGo(id) }, variant = if (last) ButtonVariant.Secondary else ButtonVariant.Quiet, size = ButtonSize.Sm)
        }
    }
}

/** A list row for a folder or file ([place]: its part of the group in a lazy list); phones get one "Actions for …" button. */
@Composable
fun NodeRow(api: SquirrelApi, node: NodeDto, progress: Float?, onClick: () -> Unit, onActions: (() -> Unit)?, place: RowPlace? = null) {
    val folder = node.kind == NodeKind.folder
    val meta = if (folder) "Folder" else "${formatBytes(node.propertySize)} · ${shortDate(node.modifiedAt)}"
    val thumb = node.thumbnail
    ListRow(
        name = node.name,
        icon = if (folder) SquirrelIcon.Folder else fileIcon(node.mime),
        meta = meta,
        onClick = onClick,
        place = place,
        leading = thumb?.let {
            {
                Thumbnail(
                    photoId = node.id,
                    url = api.thumbnailUrl(node.id, it.version),
                    blurhash = it.blurhash,
                    contentDescription = null,
                    modifier = Modifier.size(RowTile).clip(Shapes.control),
                )
            }
        },
        trailing = {
            if (progress != null) Progress(progress, label = "Downloading ${node.name}", modifier = Modifier.width(Tokens.space7))
            if (onActions != null) IconButton(SquirrelIcon.More, label = "Actions for ${node.name}", onClick = onActions)
        },
    )
}
