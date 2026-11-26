package app.squirrel.ui.sharing

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import app.squirrel.api.models.NodeKind
import app.squirrel.api.models.SharedByMeDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.ListRow
import app.squirrel.ui.explore.photoCount
import app.squirrel.ui.files.fileIcon
import app.squirrel.ui.photos.CoverCard

/** A group inside a Shared section: "Folders and files", "Albums". */
@Composable
internal fun SharedGroupHead(title: String) {
    BasicText(title, style = SquirrelTheme.type.label, modifier = Modifier.padding(top = Tokens.space2).semantics { heading() })
}

/** What an empty Shared section says. */
@Composable
internal fun SharedSectionEmpty(line: String) {
    BasicText(line, style = SquirrelTheme.type.body.copy(color = SquirrelTheme.scheme.inkMuted))
}

/** One of the caller's shared folders or files: who has it, and its Share sheet. */
@Composable
internal fun SharedByMeNodeRow(item: SharedByMeDto, onOpen: () -> Unit, onManage: () -> Unit) {
    val node = item.node ?: return
    val folder = node.kind == NodeKind.folder
    ListRow(
        name = node.name,
        icon = if (folder) SquirrelIcon.Folder else fileIcon(node.mime),
        meta = "${if (folder) "Folder" else "File"} · ${sharedWithSummary(item.people.map { it.person })}",
        onClick = onOpen,
        trailing = { IconButton(SquirrelIcon.Share, label = "Manage sharing of ${node.name}", onClick = onManage) },
    )
}

/** One of the caller's shared albums: its cover card, who has it, and its Share sheet. */
@Composable
internal fun SharedByMeAlbumCard(api: SquirrelApi, item: SharedByMeDto, modifier: Modifier, onOpen: () -> Unit, onManage: () -> Unit) {
    val album = item.album ?: return
    CoverCard(api, album.cover, album.name, photoCount(album.itemCount), onClick = onOpen, modifier = modifier, extra = {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Tokens.space1)) {
            BasicText(
                sharedWithSummary(item.people.map { it.person }),
                style = SquirrelTheme.type.caption,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            IconButton(SquirrelIcon.Share, label = "Manage sharing of ${album.name}", onClick = onManage)
        }
    })
}
