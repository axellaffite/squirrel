package app.squirrel.ui.sharing

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.SharedByMeDto
import app.squirrel.api.models.SharedItemDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.api.okOrThrow
import app.squirrel.ui.ResumeThrottle
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.FileOpener
import io.github.axellaffite.lsdo.combineState
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class SharedState(
    /** Shared with the caller: null until loaded; newest first. */
    val items: List<SharedItemDto>? = null,
    /** The caller's own shared things: null until loaded; most recently shared first. */
    val byMe: List<SharedByMeDto>? = null,
    val loading: Boolean = false,
    val error: String? = null,
    /** The item waiting for "Leave “…”?". */
    val leaving: SharedItemDto? = null,
    /** One of the caller's things whose Share sheet is open. */
    val managing: SharedByMeDto? = null,
) {
    val refreshing: Boolean get() = loading && (items != null || byMe != null)
    val nodes: List<SharedItemDto> get() = items.orEmpty().filter { it.node != null }
    val albums: List<SharedItemDto> get() = items.orEmpty().filter { it.album != null }
    val byMeNodes: List<SharedByMeDto> get() = byMe.orEmpty().filter { it.node != null }
    val byMeAlbums: List<SharedByMeDto> get() = byMe.orEmpty().filter { it.album != null }
}

/** What a Shared row opens: a folder or file, or an album (shared with the caller or by them). */
data class SharedTarget(val node: NodeDto?, val album: AlbumDto?)

val SharedItemDto.target: SharedTarget get() = SharedTarget(node, album)
val SharedByMeDto.target: SharedTarget get() = SharedTarget(node, album)

/** A shared item's name: its folder, file or album. */
fun subjectName(item: SharedItemDto): String = item.node?.name ?: item.album?.name ?: ""

fun subjectName(item: SharedByMeDto): String = item.node?.name ?: item.album?.name ?: ""

/** What the Share sheet of one of the caller's things is about. */
fun subjectOf(item: SharedByMeDto): Subject? = item.node?.let { Subject.Node(it.id) } ?: item.album?.let { Subject.Album(it.id) }

/**
 * The Shared section: folders, files and albums other people shared with the caller (and leaving
 * them), and the caller's own shared ones with their people (and managing who has them).
 */
class SharedViewModel(
    private val api: SquirrelApi,
    handoff: FileHandoff? = null,
    private val throttle: ResumeThrottle = ResumeThrottle(),
) : ViewModel() {
    private val _state = MutableStateFlow(SharedState())
    val state: StateFlow<SharedState> = _state.asStateFlow()

    /** A shared file that is not a photo opens with another app. */
    val opener = FileOpener(api, handoff, viewModelScope)

    /** The list as LS-DO shows it (see [SharedListLsDo]); one flow for the session, however often the list is shown. */
    val listing: StateFlow<SharedListState> = combineState(state, opener.state, ::sharedListState)

    init {
        load()
    }

    fun load() {
        if (_state.value.loading) return
        throttle.loaded()
        _state.update { it.copy(loading = true) }
        viewModelScope.launch {
            val withMe = async { call { api.sharing.sharedWithMe().bodyOrThrow() } }
            val byMe = async { call { api.sharing.sharedByMe().bodyOrThrow() } }
            val (withMeResult, byMeResult) = withMe.await() to byMe.await()
            _state.update { s ->
                s.copy(
                    items = (withMeResult as? Outcome.Ok)?.value ?: s.items,
                    byMe = (byMeResult as? Outcome.Ok)?.value ?: s.byMe,
                    loading = false,
                    error = (withMeResult as? Outcome.Failed)?.message ?: (byMeResult as? Outcome.Failed)?.message,
                )
            }
        }
    }

    /** Opens the Share sheet of one of the caller's things. */
    fun manage(item: SharedByMeDto) = _state.update { it.copy(managing = item) }

    /** The Share sheet closed: who has it may have changed. */
    fun doneManaging() {
        _state.update { it.copy(managing = null) }
        load()
    }

    fun resumed() {
        if (throttle.due()) load()
    }

    fun askLeave(item: SharedItemDto) = _state.update { it.copy(leaving = item) }

    fun cancelLeave() = _state.update { it.copy(leaving = null) }

    fun confirmLeave() {
        viewModelScope.launch { leave() }
    }

    /** Leaves the share asked about, once the server agreed; the server's refusal, or null. */
    suspend fun leave(): String? {
        val item = _state.value.leaving ?: return "Nothing is waiting to be left."
        val result = call { api.sharing.deleteShare(item.shareId).okOrThrow() }
        _state.update { s ->
            when (result) {
                is Outcome.Ok -> s.copy(leaving = null, items = s.items.orEmpty().filterNot { it.shareId == item.shareId })
                is Outcome.Failed -> s.copy(leaving = null, error = result.message)
            }
        }
        return (result as? Outcome.Failed)?.message
    }
}
