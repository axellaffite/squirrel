package app.squirrel.ui.sharing

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
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
import app.squirrel.api.models.SharedByMeDto
import app.squirrel.api.models.SharedItemDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.Badge
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonVariant
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

/** The "Shared" page: what other people shared with the caller, then what the caller shared. */
@Composable
fun SharedList(api: SquirrelApi, vm: SharedViewModel, onFriends: () -> Unit, onOpen: (SharedTarget) -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    var actions by remember { mutableStateOf<SharedItemDto?>(null) }
    OnForeground(vm::resumed)
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader("Shared") { IconButton(SquirrelIcon.Users, label = "Friends", onClick = onFriends) }
        val opener by vm.opener.state.collectAsStateWithLifecycle()
        opener.message?.let { Alert(it) }
        when {
            state.items == null && state.byMe == null && state.error != null -> PullableColumn(state.loading, vm::load) { ErrorRetry(state.error.orEmpty(), vm::load) }
            state.items == null && state.byMe == null -> Loading()
            else -> PullToRefresh(state.refreshing, vm::load) {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(Tokens.space2),
                    contentPadding = PaddingValues(bottom = RailClearance),
                ) {
                    state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::load) } }
                    sharedWithMe(api, state, onOpen, onActions = { actions = it })
                    sharedByMe(api, state, onOpen, onManage = vm::manage)
                }
            }
        }
    }
    actions?.let { item ->
        val name = subjectName(item)
        ActionsSheet(
            title = name,
            actions = listOf(
                SheetAction("Open", if (item.album != null) SquirrelIcon.Library else SquirrelIcon.ExternalLink, "Open $name") { onOpen(item.target) },
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
    state.managing?.let { item ->
        subjectOf(item)?.let { subject -> ShareSheet(api, subject, subjectName(item), onClose = vm::doneManaging) }
    }
}

/** "Shared with me": folders and files, then albums; a line while there is nothing. */
private fun LazyListScope.sharedWithMe(api: SquirrelApi, state: SharedState, onOpen: (SharedTarget) -> Unit, onActions: (SharedItemDto) -> Unit) {
    item(key = "with-me-head") { SectionHead("Shared with me", null) }
    val items = state.items ?: return
    if (items.isEmpty()) item(key = "with-me-empty") { SharedSectionEmpty("When someone shares a folder, a file or an album with you, it shows up here.") }
    if (state.nodes.isNotEmpty()) item(key = "with-me-files") { SharedGroupHead("Folders and files") }
    items(state.nodes, key = { "with-me-${it.shareId}" }) { item -> SharedNodeRow(item, onOpen = { onOpen(item.target) }, onActions = { onActions(item) }) }
    if (state.albums.isNotEmpty()) {
        item(key = "with-me-albums") {
            Column(verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
                SharedGroupHead("Albums")
                TwoColumns(state.albums) { item, modifier -> SharedAlbumCard(api, item, modifier, onOpen = { onOpen(item.target) }, onActions = { onActions(item) }) }
            }
        }
    }
}

/** "Shared by me": the caller's folders and files, then albums, each with who has it. */
private fun LazyListScope.sharedByMe(api: SquirrelApi, state: SharedState, onOpen: (SharedTarget) -> Unit, onManage: (SharedByMeDto) -> Unit) {
    item(key = "by-me-head") { SectionHead("Shared by me", null) }
    val items = state.byMe ?: return
    if (items.isEmpty()) item(key = "by-me-empty") { SharedSectionEmpty("Share a folder, a file or an album and it shows up here, with the people you shared it with.") }
    if (state.byMeNodes.isNotEmpty()) item(key = "by-me-files") { SharedGroupHead("Folders and files") }
    items(state.byMeNodes, key = { "by-me-${it.node?.id}" }) { item -> SharedByMeNodeRow(item, onOpen = { onOpen(item.target) }, onManage = { onManage(item) }) }
    if (state.byMeAlbums.isNotEmpty()) {
        item(key = "by-me-albums") {
            Column(verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
                SharedGroupHead("Albums")
                TwoColumns(state.byMeAlbums) { item, modifier -> SharedByMeAlbumCard(api, item, modifier, onOpen = { onOpen(item.target) }, onManage = { onManage(item) }) }
            }
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
