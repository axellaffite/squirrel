package app.squirrel.ui.files

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.AccessLevel
import app.squirrel.api.models.CreateFolderRequest
import app.squirrel.api.models.MemberDto
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.NodeKind
import app.squirrel.api.models.NodeListing
import app.squirrel.api.models.PathSegmentDto
import app.squirrel.api.models.UpdateNodeRequest
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.sync.formatBytes
import app.squirrel.ui.ResumeThrottle
import app.squirrel.ui.attempt
import app.squirrel.ui.photos.UNREACHABLE
import app.squirrel.ui.sharing.Abilities
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.call
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable

/** Files over this size ask before downloading. */
const val CONFIRM_BYTES = 200L * 1000 * 1000

@Serializable
data class FilesState(
    /** Null: the root ("Files"). */
    val folderId: String? = null,
    /** The folder itself, once its details arrived (null at the root). */
    val folder: NodeDto? = null,
    /** What the caller may do here: the top level is always theirs. */
    val access: AccessLevel = AccessLevel.owner,
    /** Who shared it with the caller; null for their own. */
    val sharedBy: MemberDto? = null,
    /** Breadcrumbs from the root to the current folder. */
    val path: List<PathSegmentDto> = emptyList(),
    val items: List<NodeDto> = emptyList(),
    val nextCursor: String? = null,
    val started: Boolean = false,
    val loading: Boolean = false,
    /** Reloading the folder: the rows stay until the new first page replaces them. */
    val refreshing: Boolean = false,
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

    /** The folder's first page has not arrived yet: the screen shows its loading rows. */
    val firstLoad: Boolean get() = loading && !started

    val can: Abilities get() = Abilities.of(access)

    /** Files only live inside a folder: the top level is fixed. */
    val canAdd: Boolean get() = folderId != null && folder != null && can.edit
}

/**
 * The file browser: folders, breadcrumbs, keyset paging, open with another app. It starts at
 * [root] (null: the top level; a shared folder in Shared) and never goes up past it.
 */
class FilesViewModel(
    private val api: SquirrelApi,
    private val handoff: FileHandoff?,
    private val pageSize: Int = 100,
    private val throttle: ResumeThrottle = ResumeThrottle(),
    /** Where the browser starts and never goes up past (null: the top level). */
    val root: String? = null,
) : ViewModel(), LsDoScreen<FilesState, FilesIntent> {
    private val _state = MutableStateFlow(FilesState())
    override val state: StateFlow<FilesState> = _state.asStateFlow()

    override val lsdo = lsdoSpec<FilesState, FilesIntent>()

    override suspend fun perform(intent: FilesIntent): String? = performIntent(intent)

    override fun availableIntents(state: FilesState): Set<String> = intentsFor(state)

    override fun isBusy(state: FilesState): Boolean = state.loading || state.refreshing

    /** The folder load or page in flight; a new folder cancels it. */
    private var loadJob: Job? = null

    /** Bumped on every folder change and refresh: late answers for an older listing are dropped. */
    private var generation = 0

    /** The last refresh failed: "Try again" refreshes again (rather than paging on). */
    private var refreshFailed = false

    init {
        enter(root)
    }

    /** Opens a folder ([id] null: the root). */
    fun enter(id: String?) {
        loadJob?.cancel()
        val gen = ++generation
        refreshFailed = false
        throttle.loaded()
        _state.update { FilesState(folderId = id, loading = true, downloads = it.downloads) }
        loadJob = viewModelScope.launch {
            try {
                val details = if (id == null) null else attempt { api.files.getNode(id).bodyOrThrow() }
                if (id != null && details == null) return@launch update(gen) { it.copy(error = UNREACHABLE) }
                // The node's path lists its ancestors only: the folder itself ends the breadcrumbs.
                val path = details?.let { it.path + PathSegmentDto(it.node.id, it.node.name) }.orEmpty()
                update(gen) {
                    it.copy(path = path, folder = details?.node, access = details?.access ?: AccessLevel.owner, sharedBy = details?.sharedBy)
                }
                page(gen, id, cursor = null)
            } finally {
                update(gen) { it.copy(loading = false) }
            }
        }
    }

    /** Whether this is where the browser started (no "up" from here). */
    val atRoot: Boolean get() = _state.value.folderId == root

    /** One level up; false at the root (the caller leaves the tab's back handling alone). */
    fun up(): Boolean {
        val current = _state.value
        if (current.folderId == root) return false
        enter(current.path.dropLast(1).lastOrNull()?.id ?: root)
        return true
    }

    /** The next page; never while the folder itself is still loading. */
    fun more() {
        val current = _state.value
        // `loading`, not the job: a load clears it in `finally`, while its job is still active.
        if (current.loading || current.complete || current.error != null || !current.started) return
        val gen = generation
        _state.update { it.copy(loading = true) }
        loadJob = viewModelScope.launch {
            try {
                page(gen, current.folderId, current.nextCursor)
            } finally {
                update(gen) { it.copy(loading = false) }
            }
        }
    }

    /**
     * The current folder again, from its first page. The rows stay until that page arrives; a
     * page in flight is cancelled and its answer dropped, so the two never mix.
     */
    fun refresh() {
        val current = _state.value
        if (current.refreshing || current.firstLoad) return
        if (!current.started) return enter(current.folderId)
        loadJob?.cancel()
        val gen = ++generation
        throttle.loaded()
        _state.update { it.copy(loading = true, refreshing = true, error = null) }
        loadJob = viewModelScope.launch {
            var listing: NodeListing? = null
            try {
                listing = attempt { api.files.listNodes(parentId = current.folderId, cursor = null, limit = pageSize).bodyOrThrow() }
            } finally {
                val fresh = listing
                if (gen == generation) refreshFailed = fresh == null
                update(gen) { state ->
                    val settled = state.copy(loading = false, refreshing = false)
                    if (fresh == null) settled.copy(error = UNREACHABLE)
                    else settled.copy(items = fresh.items.distinctBy { it.id }, nextCursor = fresh.nextCursor)
                }
            }
        }
    }

    /** The app came back: a refresh, unless the folder was loaded moments ago. */
    fun resumed() {
        if (_state.value.started && throttle.due()) refresh()
    }

    fun retry() {
        val current = _state.value
        if (!current.started) {
            enter(current.folderId)
        } else if (refreshFailed) {
            refresh()
        } else {
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
            val result = try {
                handoff.open(api, node.id, node.name, node.mime, share = false) { progress ->
                    _state.update { s -> if (node.id in s.downloads) s.copy(downloads = s.downloads + (node.id to progress)) else s }
                }
            } finally {
                _state.update { it.copy(downloads = it.downloads - node.id) }
            }
            _state.update {
                it.copy(
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

    /** "New folder" in the folder shown; an error message, or null once it is listed. */
    suspend fun createFolder(name: String): String? {
        val parent = _state.value.folderId ?: return "Open a folder to add one."
        return edited { api.files.createFolder(CreateFolderRequest(name = name, parentId = parent)).bodyOrThrow() }
    }

    suspend fun rename(node: NodeDto, name: String): String? =
        edited { api.files.updateNode(node.id, UpdateNodeRequest(name = name)).bodyOrThrow() }

    suspend fun move(node: NodeDto, parentId: String): String? =
        edited { api.files.updateNode(node.id, UpdateNodeRequest(parentId = parentId)).bodyOrThrow() }

    /** Runs a change, then shows the folder again; the server's refusal is the message. */
    private suspend fun edited(change: suspend () -> Unit): String? = when (val result = call { change() }) {
        is Outcome.Ok -> null.also { refresh() }
        is Outcome.Failed -> result.message
    }

    private suspend fun page(gen: Int, folder: String?, cursor: String?) {
        val listing = attempt { api.files.listNodes(parentId = folder, cursor = cursor, limit = pageSize).bodyOrThrow() }
        update(gen) { state ->
            if (listing == null) {
                state.copy(error = UNREACHABLE)
            } else {
                state.copy(
                    // A row that moved between two pages arrives twice: keep its first place.
                    items = (state.items + listing.items).distinctBy { it.id },
                    nextCursor = listing.nextCursor,
                    started = true,
                )
            }
        }
    }

    /** Applies [change] only while [gen] is still the current folder. */
    private fun update(gen: Int, change: (FilesState) -> FilesState) {
        _state.update { if (gen == generation) change(it) else it }
    }
}
