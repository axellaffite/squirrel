package app.squirrel.ui.files

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.NodeKind
import app.squirrel.api.models.PathSegmentDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.sync.formatBytes
import app.squirrel.ui.photos.UNREACHABLE
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Files over this size ask before downloading. */
const val CONFIRM_BYTES = 200L * 1000 * 1000

data class FilesState(
    /** Null: the root ("Files"). */
    val folderId: String? = null,
    /** Breadcrumbs from the root to the current folder (`GET /api/nodes/{id}` path). */
    val path: List<PathSegmentDto> = emptyList(),
    val items: List<NodeDto> = emptyList(),
    val nextCursor: String? = null,
    val started: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
    /** Download progress (0–1) per file id. */
    val downloads: Map<String, Float> = emptyMap(),
    /** A large file waiting for "Download 1.4 GB?". */
    val confirm: NodeDto? = null,
    val message: String? = null,
) {
    val complete: Boolean get() = started && nextCursor == null
    val title: String get() = path.lastOrNull()?.name ?: "Files"
    val confirmText: String? get() = confirm?.let { "Download ${formatBytes(it.propertySize)}?" }
}

/** The read-only file browser: folders, breadcrumbs, keyset paging, open with another app. */
class FilesViewModel(
    private val api: SquirrelApi,
    private val handoff: FileHandoff?,
    private val pageSize: Int = 100,
) : ViewModel() {
    private val _state = MutableStateFlow(FilesState())
    val state: StateFlow<FilesState> = _state.asStateFlow()

    init {
        enter(null)
    }

    /** Opens a folder ([id] null: the root). */
    fun enter(id: String?) {
        _state.value = FilesState(folderId = id, loading = true)
        viewModelScope.launch {
            // The node's path lists its ancestors only: the folder itself ends the breadcrumbs.
            val path = if (id == null) emptyList() else attempt {
                val details = api.files.getNode(id).bodyOrThrow()
                details.path + PathSegmentDto(details.node.id, details.node.name)
            }
            if (path == null) return@launch _state.update { it.copy(loading = false, error = UNREACHABLE) }
            _state.update { it.copy(path = path) }
            page(id, cursor = null)
        }
    }

    /** One level up; false at the root (the caller leaves the tab's back handling alone). */
    fun up(): Boolean {
        val path = _state.value.path
        if (_state.value.folderId == null) return false
        enter(path.dropLast(1).lastOrNull()?.id)
        return true
    }

    fun more() {
        val current = _state.value
        if (current.loading || current.complete || current.error != null) return
        _state.update { it.copy(loading = true) }
        viewModelScope.launch { page(current.folderId, current.nextCursor) }
    }

    fun retry() {
        val current = _state.value
        if (!current.started) enter(current.folderId) else {
            _state.update { it.copy(error = null) }
            more()
        }
    }

    fun open(node: NodeDto, confirmed: Boolean = false) {
        if (node.kind == NodeKind.folder) return enter(node.id)
        val handoff = handoff ?: return
        if (node.id in _state.value.downloads) return
        if (!confirmed && node.propertySize > CONFIRM_BYTES) return _state.update { it.copy(confirm = node) }
        _state.update { it.copy(confirm = null, message = null, downloads = it.downloads + (node.id to 0f)) }
        viewModelScope.launch {
            val result = handoff.open(api, node.id, node.name, node.mime, share = false) { progress ->
                _state.update { s -> if (node.id in s.downloads) s.copy(downloads = s.downloads + (node.id to progress)) else s }
            }
            _state.update {
                it.copy(
                    downloads = it.downloads - node.id,
                    message = when (result) {
                        HandoffResult.Opened -> null
                        HandoffResult.NoApp -> "No app on this phone opens ${node.name}."
                        is HandoffResult.Failed -> result.message
                    },
                )
            }
        }
    }

    fun dismissConfirm() = _state.update { it.copy(confirm = null) }

    private suspend fun page(folder: String?, cursor: String?) {
        val listing = attempt { api.files.listNodes(parentId = folder, cursor = cursor, limit = pageSize).bodyOrThrow() }
        _state.update { state ->
            if (state.folderId != folder) return@update state // the user moved on meanwhile
            if (listing == null) state.copy(loading = false, error = UNREACHABLE)
            else state.copy(items = state.items + listing.items, nextCursor = listing.nextCursor, started = true, loading = false)
        }
    }

    private suspend fun <T> attempt(call: suspend () -> T): T? = try {
        call()
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }
}
