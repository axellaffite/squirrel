package app.squirrel.ui.files

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.NodeKind
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.sync.formatBytes
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.Icon
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.Progress
import app.squirrel.theme.components.PullToRefresh
import app.squirrel.theme.components.Sheet
import app.squirrel.ui.AppGraph
import app.squirrel.ui.OnForeground
import app.squirrel.ui.PullableColumn
import app.squirrel.ui.images.Thumbnail
import app.squirrel.ui.photos.ErrorRetry
import app.squirrel.ui.photos.RailClearance
import app.squirrel.ui.photos.monthLabel
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import kotlinx.datetime.TimeZone
import kotlinx.datetime.toLocalDateTime

/** Files: folders with breadcrumbs, row cards, open a file with another app. Read-only. */
@Composable
fun FilesTab(graph: AppGraph) {
    val api = graph.api ?: return
    val vm = viewModel(key = "files") { FilesViewModel(api, graph.files) }
    val state by vm.state.collectAsStateWithLifecycle()
    OnForeground(vm::resumed)
    SystemBack(enabled = state.folderId != null) { vm.up() }
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader(state.title, onBack = if (state.folderId != null) ({ vm.up() }) else null, backLabel = "Up one folder")
        if (state.path.isNotEmpty()) Breadcrumbs(state.path.map { it.id to it.name }, vm::enter)
        state.message?.let { Alert(it) }
        when {
            state.error != null && state.items.isEmpty() ->
                PullableColumn(state.refreshing, vm::refresh) { ErrorRetry(checkNotNull(state.error), vm::retry) }
            state.firstLoad -> Loading()
            state.complete && state.items.isEmpty() -> PullableColumn(state.refreshing, vm::refresh) {
                EmptyState(SquirrelIcon.Folder, "Nothing here yet", "Files you add on the web or back up from this phone appear here.")
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
                        NodeRow(api, node, state.downloads[node.id]) { vm.open(node) }
                    }
                    state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::retry) } }
                }
            }
        }
    }
    state.confirm?.let { big ->
        Sheet(state.confirmText.orEmpty(), onDismiss = vm::dismissConfirm) {
            BasicText("${big.name} is large: it is downloaded to this phone before it opens.", style = SquirrelTheme.type.body)
            Button("Download", onClick = { vm.open(big, confirmed = true) }, full = true)
            Button("Cancel", onClick = vm::dismissConfirm, variant = ButtonVariant.Quiet, full = true)
        }
    }
}

/** Pills from "Files" to the current folder; each goes back there. */
@Composable
private fun Breadcrumbs(path: List<Pair<String, String>>, onGo: (String?) -> Unit) {
    Row(
        modifier = Modifier.horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space1),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Button("Files", onClick = { onGo(null) }, variant = ButtonVariant.Quiet, size = ButtonSize.Sm)
        path.forEachIndexed { index, (id, name) ->
            Icon(SquirrelIcon.ChevronRight, contentDescription = null, tint = SquirrelTheme.scheme.inkMuted)
            val last = index == path.lastIndex
            Button(name, onClick = { if (!last) onGo(id) }, variant = if (last) ButtonVariant.Secondary else ButtonVariant.Quiet, size = ButtonSize.Sm)
        }
    }
}

@Composable
private fun NodeRow(api: SquirrelApi, node: NodeDto, progress: Float?, onClick: () -> Unit) {
    val folder = node.kind == NodeKind.folder
    val meta = if (folder) "Folder" else "${formatBytes(node.propertySize)} · ${dateLabel(node)}"
    val thumb = node.thumbnail
    ListRow(
        name = node.name,
        icon = if (folder) SquirrelIcon.Folder else fileIcon(node.mime),
        meta = meta,
        onClick = onClick,
        leading = thumb?.let {
            {
                Thumbnail(
                    url = api.thumbnailUrl(node.id, it.version),
                    blurhash = it.blurhash,
                    contentDescription = null,
                    modifier = Modifier.size(Tokens.wellSm).clip(Shapes.circle),
                )
            }
        },
        trailing = when {
            progress != null -> ({ Progress(progress, label = "Downloading ${node.name}", modifier = Modifier.width(Tokens.space7)) })
            folder -> ({ Icon(SquirrelIcon.ChevronRight, contentDescription = null, tint = SquirrelTheme.scheme.inkMuted) })
            else -> null
        },
    )
}

private fun fileIcon(mime: String?): SquirrelIcon = when {
    mime == null -> SquirrelIcon.File
    mime.startsWith("image/") -> SquirrelIcon.Image
    mime.startsWith("video/") -> SquirrelIcon.Film
    mime.startsWith("audio/") -> SquirrelIcon.Music
    mime == "application/pdf" || mime.startsWith("text/") -> SquirrelIcon.FileText
    else -> SquirrelIcon.File
}

private fun dateLabel(node: NodeDto): String {
    val date = node.modifiedAt.toLocalDateTime(TimeZone.currentSystemDefault()).date
    return "${date.day} ${monthLabel("${date.year}-${date.month.ordinal + 1}")}"
}
