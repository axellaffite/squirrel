package app.squirrel.ui.sharing

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.NodeKind
import app.squirrel.api.models.SharedItemDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.Badge
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.PullToRefresh
import app.squirrel.theme.components.Sheet
import app.squirrel.ui.OnForeground
import app.squirrel.ui.PullableColumn
import app.squirrel.ui.explore.SectionHead
import app.squirrel.ui.files.fileIcon
import app.squirrel.ui.photos.CoverCard
import app.squirrel.ui.photos.ErrorRetry
import app.squirrel.ui.photos.RailClearance
import app.squirrel.ui.photos.TwoColumns
import app.squirrel.ui.explore.photoCount
import app.squirrel.ui.shell.ActionsSheet
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SheetAction

/** The "Shared" page: folders, files and albums other people shared with the caller. */
@Composable
fun SharedList(api: SquirrelApi, vm: SharedViewModel, onOpen: (SharedItemDto) -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    var actions by remember { mutableStateOf<SharedItemDto?>(null) }
    OnForeground(vm::resumed)
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader("Shared")
        val opener by vm.opener.state.collectAsStateWithLifecycle()
        opener.message?.let { Alert(it) }
        val items = state.items
        when {
            items == null && state.error != null -> PullableColumn(state.loading, vm::load) { ErrorRetry(state.error.orEmpty(), vm::load) }
            items == null -> Loading()
            items.isEmpty() -> PullableColumn(state.refreshing, vm::load) {
                state.error?.let { ErrorRetry(it, vm::load) }
                EmptyState(SquirrelIcon.Users, "Nothing shared with you yet", "When someone shares a folder, a file or an album with you, it shows up here.")
            }
            else -> PullToRefresh(state.refreshing, vm::load) {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(Tokens.space2),
                    contentPadding = PaddingValues(bottom = RailClearance),
                ) {
                    state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::load) } }
                    if (state.nodes.isNotEmpty()) item(key = "files-head") { SectionHead("Folders and files", null) }
                    items(state.nodes, key = { it.shareId }) { item -> SharedNodeRow(item, onOpen = { onOpen(item) }, onActions = { actions = item }) }
                    if (state.albums.isNotEmpty()) {
                        item(key = "albums") {
                            Column(verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
                                SectionHead("Albums", null)
                                TwoColumns(state.albums) { item, modifier -> SharedAlbumCard(api, item, modifier, onOpen = { onOpen(item) }, onActions = { actions = item }) }
                            }
                        }
                    }
                }
            }
        }
    }
    actions?.let { item ->
        val name = subjectName(item)
        ActionsSheet(
            title = name,
            actions = listOf(
                SheetAction("Open", if (item.album != null) SquirrelIcon.Library else SquirrelIcon.ExternalLink, "Open $name") { onOpen(item) },
                SheetAction("Leave", SquirrelIcon.LogOut, "Leave $name", danger = true) { vm.askLeave(item) },
            ),
            onClose = { actions = null },
        )
    }
    state.leaving?.let { item ->
        Sheet("Leave “${subjectName(item)}”?", onDismiss = vm::cancelLeave) {
            BasicText("You will no longer see it. ${memberName(item.owner)} can share it with you again.", style = SquirrelTheme.type.body)
            Button("Leave", onClick = vm::confirmLeave, variant = ButtonVariant.DangerSolid, full = true)
            Button("Cancel", onClick = vm::cancelLeave, variant = ButtonVariant.Quiet, full = true)
        }
    }
}

@Composable
private fun SharedNodeRow(item: SharedItemDto, onOpen: () -> Unit, onActions: () -> Unit) {
    val node = item.node ?: return
    val folder = node.kind == NodeKind.folder
    ListRow(
        name = node.name,
        icon = if (folder) SquirrelIcon.Folder else fileIcon(node.mime),
        meta = "${if (folder) "Folder" else "File"} · ${memberName(item.owner)}",
        onClick = onOpen,
        trailing = {
            Badge(permissionLabel(item.permission))
            IconButton(SquirrelIcon.More, label = "Actions for ${node.name}", onClick = onActions)
        },
    )
}

@Composable
private fun SharedAlbumCard(api: SquirrelApi, item: SharedItemDto, modifier: Modifier, onOpen: () -> Unit, onActions: () -> Unit) {
    val album = item.album ?: return
    CoverCard(api, album.cover, album.name, photoCount(album.itemCount), onClick = onOpen, modifier = modifier, extra = {
        BasicText(memberName(item.owner), style = SquirrelTheme.type.caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Row(verticalAlignment = Alignment.CenterVertically) {
            Badge(permissionLabel(item.permission))
            Spacer(Modifier.weight(1f))
            IconButton(SquirrelIcon.More, label = "Actions for ${album.name}", onClick = onActions)
        }
    })
}
