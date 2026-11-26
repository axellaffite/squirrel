package app.squirrel.ui.files

import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.NodeKind
import app.squirrel.api.models.PathSegmentDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.Icon
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.RowGroup
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.Sheet
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.call
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Folders a move dialog lists per request (homes hold at most a few thousand). */
private const val FOLDER_PAGE = 500

data class MoveState(
    /** The folders from where browsing started to the one shown; empty: the top level. */
    val trail: List<PathSegmentDto> = emptyList(),
    val folders: List<NodeDto> = emptyList(),
    val loading: Boolean = true,
    val error: String? = null,
) {
    val here: PathSegmentDto? get() = trail.lastOrNull()
}

/**
 * The folders a node can move to, browsed from [root] (the share, for people who do not own the
 * folder; the top level otherwise) and never above it. The node itself is never offered. Without
 * a [node], any folder can be chosen (where to send files).
 */
class MoveModel(private val api: SquirrelApi, private val node: NodeDto?, private val root: PathSegmentDto?, private val scope: CoroutineScope) {
    private val start = listOfNotNull(root)
    private val _state = MutableStateFlow(MoveState(trail = start))
    val state: StateFlow<MoveState> = _state.asStateFlow()
    private var job: Job? = null

    fun load() = browse(start)

    fun open(folder: NodeDto) = browse(_state.value.trail + PathSegmentDto(folder.id, folder.name))

    /** Up one folder; false at the start (nowhere to go). */
    fun up(): Boolean {
        val trail = _state.value.trail
        if (trail.size <= start.size) return false
        browse(trail.dropLast(1))
        return true
    }

    val canGoUp: Boolean get() = _state.value.trail.size > start.size

    /** Whether "Move here" may run: a folder is chosen and it is not where the node already is. */
    val canMoveHere: Boolean get() = _state.value.here?.let { node == null || it.id != node.parentId } ?: false

    private fun browse(next: List<PathSegmentDto>) {
        job?.cancel()
        _state.update { it.copy(loading = true) }
        // Clicking quickly through folders must not let an older listing win.
        job = scope.launch {
            val result = call { folders(next.lastOrNull()?.id) }
            _state.update { s ->
                when (result) {
                    is Outcome.Ok -> MoveState(trail = next, folders = result.value, loading = false)
                    is Outcome.Failed -> s.copy(loading = false, error = result.message)
                }
            }
        }
    }

    private suspend fun folders(parent: String?): List<NodeDto> {
        val out = mutableListOf<NodeDto>()
        var cursor: String? = null
        do {
            val page = api.files.listNodes(parentId = parent, cursor = cursor, limit = FOLDER_PAGE).bodyOrThrow()
            out += page.items.filter { it.kind == NodeKind.folder && it.id != node?.id }
            cursor = page.nextCursor
        } while (cursor != null)
        return out
    }
}

/** "Move “report.pdf”": browse to a folder, then "Move here". [onMove] answers an error or null. */
@Composable
fun MoveSheet(api: SquirrelApi, node: NodeDto, root: PathSegmentDto?, onMove: suspend (String) -> String?, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    val model = remember(node.id) { MoveModel(api, node, root, scope) }
    FolderPickerSheet(model, "Move “${node.name}”", "Move here", onPick = { onMove(it.last().id) }, onClose = onClose)
}

/**
 * A sheet to browse to a folder of one's own, then [confirm] ("Send here"): [onPick] gets the path
 * to it, top level first, and answers an error or null (the sheet then closes).
 */
@Composable
fun FolderPickerSheet(api: SquirrelApi, title: String, confirm: String, onPick: suspend (List<PathSegmentDto>) -> String?, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    val model = remember { MoveModel(api, node = null, root = null, scope = scope) }
    FolderPickerSheet(model, title, confirm, onPick, onClose)
}

@Composable
private fun FolderPickerSheet(model: MoveModel, title: String, confirm: String, onPick: suspend (List<PathSegmentDto>) -> String?, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    LaunchedEffect(model) { model.load() }
    val state by model.state.collectAsStateWithLifecycle()
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    Sheet(title, onDismiss = onClose) {
        if (model.canGoUp) Button("Up", onClick = { model.up() }, variant = ButtonVariant.Quiet, size = ButtonSize.Sm, icon = SquirrelIcon.ChevronLeft)
        val where = state.here?.let { "To: ${state.trail.joinToString(" › ") { it.name }}" } ?: "Choose a folder"
        BasicText(where, style = SquirrelTheme.type.label, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
        if (state.folders.isNotEmpty()) RowGroup {
            state.folders.forEach { folder ->
                ListRow(folder.name, icon = SquirrelIcon.Folder, onClick = { model.open(folder) }, trailing = {
                    Icon(SquirrelIcon.ChevronRight, contentDescription = null, tint = SquirrelTheme.scheme.inkMuted)
                })
            }
        }
        when {
            state.loading -> Loading()
            state.folders.isEmpty() && state.error == null -> BasicText("No subfolders", style = SquirrelTheme.type.caption)
        }
        (error ?: state.error)?.let { Alert(it, tone = AlertTone.Error) }
        Button(confirm, enabled = !busy && model.canMoveHere, full = true, onClick = {
            val trail = state.trail.takeIf { it.isNotEmpty() } ?: return@Button
            busy = true
            scope.launch {
                val problem = try { onPick(trail) } finally { busy = false }
                if (problem == null) onClose() else error = problem
            }
        })
        Button("Cancel", onClick = onClose, variant = ButtonVariant.Quiet, full = true)
    }
}
