package app.squirrel.ui.files

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.NodeKind
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.Card
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.Progress
import app.squirrel.theme.components.PullToRefresh
import app.squirrel.theme.components.Sheet
import app.squirrel.ui.AppGraph
import app.squirrel.ui.OnForeground
import app.squirrel.ui.PullableColumn
import app.squirrel.ui.photos.ErrorRetry
import app.squirrel.ui.photos.RailClearance
import app.squirrel.ui.sharing.Abilities
import app.squirrel.ui.sharing.CommentsSheet
import app.squirrel.ui.sharing.ShareSheet
import app.squirrel.ui.sharing.Subject
import app.squirrel.ui.sharing.memberName
import app.squirrel.ui.sharing.moveRoot
import app.squirrel.ui.shell.ActionsSheet
import app.squirrel.ui.shell.NameSheet
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SheetAction
import app.squirrel.ui.shell.SystemBack
import io.github.axellaffite.lsdo.compose.RegisterLsDo
import kotlinx.coroutines.launch

/** The sheet a folder page shows over its rows. */
private sealed interface FolderSheet {
    data class Actions(val node: NodeDto) : FolderSheet
    data class Share(val node: NodeDto) : FolderSheet
    data class Comments(val node: NodeDto) : FolderSheet
    data class Rename(val node: NodeDto) : FolderSheet
    data class Move(val node: NodeDto) : FolderSheet
    data object Add : FolderSheet
    data object NewFolder : FolderSheet
}

/**
 * A folder page, for one's own folders (Files) and for someone else's (Shared): breadcrumbs
 * from [rootLabel], row cards, and only the tools the folder's access allows. [onLeave] goes
 * back past the browser's first folder (Shared's list); [onSearch] adds the search action.
 */
@Composable
fun FolderBrowser(
    graph: AppGraph,
    vm: FilesViewModel,
    rootLabel: String,
    onLeave: (() -> Unit)? = null,
    onSearch: (() -> Unit)? = null,
) {
    val api = graph.api ?: return
    RegisterLsDo(rootLabel.lowercase(), vm)
    val state by vm.state.collectAsStateWithLifecycle()
    var sheet by remember { mutableStateOf<FolderSheet?>(null) }
    val uploads = viewModel(key = "uploads") { UploadsViewModel(api) }
    RegisterLsDo("uploads", uploads)
    val uploading by uploads.items.collectAsStateWithLifecycle()
    val picker = LocalFilePicker.current
    val scope = rememberCoroutineScope()
    // A file that arrived in the folder shown: the rows refresh.
    LaunchedEffect(uploads, vm) { uploads.finished.collect { if (it == vm.state.value.folderId) vm.refresh() } }
    OnForeground(vm::resumed)
    val back: (() -> Unit)? = if (!vm.atRoot) ({ vm.up() }) else onLeave
    SystemBack(enabled = back != null) { back?.invoke() }
    val can = state.can
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader(state.title, onBack = back, backLabel = if (!vm.atRoot) "Up one folder" else "Back to ${rootLabel.lowercase()}", actions = {
            if (onSearch != null) IconButton(SquirrelIcon.Search, label = "Search files", onClick = onSearch)
            state.folder?.let { folder ->
                if (state.canAdd) IconButton(SquirrelIcon.Plus, label = "Add to ${folder.name}", onClick = { sheet = FolderSheet.Add }, tinted = true)
                IconButton(SquirrelIcon.MessageCircle, label = "Comments on ${folder.name}", onClick = { sheet = FolderSheet.Comments(folder) }, tinted = true)
                if (can.owner) IconButton(SquirrelIcon.Share, label = "Share ${folder.name}", onClick = { sheet = FolderSheet.Share(folder) }, tinted = true)
            }
        })
        if (state.path.isNotEmpty()) {
            Breadcrumbs(
                rootLabel = if (can.owner) "Files" else "Shared",
                path = state.path.map { it.id to it.name },
                onRoot = { if (can.owner) vm.enter(null) else onLeave?.invoke() },
                onGo = vm::enter,
            )
        }
        state.sharedBy?.let { BasicText("Shared by ${memberName(it)}", style = SquirrelTheme.type.caption) }
        state.message?.let { Alert(it) }
        UploadRows(uploading.filter { it.parentId == state.folderId }, uploads::dismiss)
        when {
            state.error != null && state.items.isEmpty() ->
                PullableColumn(state.refreshing, vm::refresh) { ErrorRetry(checkNotNull(state.error), vm::retry) }
            state.firstLoad -> Loading()
            state.complete && state.items.isEmpty() -> PullableColumn(state.refreshing, vm::refresh) {
                EmptyState(SquirrelIcon.Folder, "Nothing here yet", if (state.canAdd) "Upload files from this phone, or add them on the web." else "Files added here show up in this list.")
            }
            else -> PullToRefresh(state.refreshing, vm::refresh) {
                LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(Tokens.space2),
                    contentPadding = PaddingValues(bottom = RailClearance),
                ) {
                    itemsIndexed(state.items, key = { _, node -> node.id }) { index, node ->
                        // The last row coming into view asks for the next page (by place, not by value).
                        if (index == state.items.lastIndex) LaunchedEffect(state.items.size) { vm.more() }
                        NodeRow(api, node, state.downloads[node.id], onClick = { vm.open(node) }, onActions = { sheet = FolderSheet.Actions(node) })
                    }
                    state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::retry) } }
                }
            }
        }
    }
    when (val current = sheet) {
        is FolderSheet.Actions -> ActionsSheet(current.node.name, nodeActions(current.node, can, vm) { sheet = it }, onClose = { sheet = null })
        is FolderSheet.Rename -> NameSheet(
            title = "Rename “${current.node.name}”",
            submitLabel = "Save",
            initial = current.node.name,
            validate = ::nameProblem,
            onSubmit = { vm.rename(current.node, it) },
            onClose = { sheet = null },
        )
        is FolderSheet.Move -> MoveSheet(api, current.node, moveRoot(state.access, state.path), onMove = { vm.move(current.node, it) }, onClose = { sheet = null })
        FolderSheet.NewFolder -> NameSheet("New folder", "Create folder", validate = ::nameProblem, onSubmit = vm::createFolder, onClose = { sheet = null })
        FolderSheet.Add -> ActionsSheet(state.title, buildList {
            val folder = state.folderId
            add(SheetAction("New folder", SquirrelIcon.FolderPlus, "New folder in ${state.title}") { sheet = FolderSheet.NewFolder })
            if (picker != null && folder != null) {
                add(SheetAction("Upload files", SquirrelIcon.Upload, "Upload files to ${state.title}") {
                    scope.launch { uploads.enqueue(picker.pick(), folder) }
                })
            }
        }, onClose = { sheet = null })
        is FolderSheet.Share -> ShareSheet(api, Subject.Node(current.node.id), current.node.name, onClose = { sheet = null })
        is FolderSheet.Comments -> CommentsSheet(api, Subject.Node(current.node.id), "“${current.node.name}”", onClose = { sheet = null })
        null -> Unit
    }
    state.confirm?.let { big ->
        Sheet(state.confirmText.orEmpty(), onDismiss = vm::dismissConfirm) {
            BasicText("${big.name} is large: it is downloaded to this phone before it opens.", style = SquirrelTheme.type.body)
            Button("Download", onClick = { vm.open(big, confirmed = true) }, full = true)
            Button("Cancel", onClick = vm::dismissConfirm, variant = ButtonVariant.Quiet, full = true)
        }
    }
}

/**
 * A row's actions: open, share (owners), comments, and rename and move where the caller may edit
 * (top-level folders stay put). Trash and delete stay on the web: phones may not destroy.
 */
private fun nodeActions(node: NodeDto, can: Abilities, vm: FilesViewModel, show: (FolderSheet) -> Unit): List<SheetAction> = buildList {
    val folder = node.kind == NodeKind.folder
    add(SheetAction(if (folder) "Open" else "Open in another app", if (folder) SquirrelIcon.Folder else SquirrelIcon.ExternalLink, "Open ${node.name}") { vm.open(node) })
    if (can.owner) add(SheetAction("Share", SquirrelIcon.Share, "Share ${node.name}") { show(FolderSheet.Share(node)) })
    add(SheetAction("Comments", SquirrelIcon.MessageCircle, "Comments on ${node.name}") { show(FolderSheet.Comments(node)) })
    if (can.edit && !node.isRoot) {
        add(SheetAction("Rename", SquirrelIcon.Pencil, "Rename ${node.name}") { show(FolderSheet.Rename(node)) })
        add(SheetAction("Move", SquirrelIcon.FolderInput, "Move ${node.name}") { show(FolderSheet.Move(node)) })
    }
}

/** Files on their way up into this folder, and the ones that failed (until dismissed). */
@Composable
private fun UploadRows(items: List<UploadItem>, onDismiss: (Int) -> Unit) {
    items.forEach { item ->
        val error = item.error
        if (error != null) {
            Alert(error, tone = AlertTone.Error) {
                Button("Dismiss", onClick = { onDismiss(item.key) }, variant = ButtonVariant.Secondary, size = ButtonSize.Sm)
            }
        } else {
            Card(Modifier.fillMaxWidth()) {
                BasicText("Uploading ${item.name}", style = SquirrelTheme.type.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Progress(item.progress, label = "Uploading ${item.name}", modifier = Modifier.padding(top = Tokens.space2))
            }
        }
    }
}
