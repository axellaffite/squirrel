package app.squirrel.ui.sharing

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.SharedItemDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.api.okOrThrow
import app.squirrel.ui.ResumeThrottle
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.FileOpener
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class SharedState(
    /** Null until loaded; newest first. */
    val items: List<SharedItemDto>? = null,
    val loading: Boolean = false,
    val error: String? = null,
    /** The item waiting for "Leave “…”?". */
    val leaving: SharedItemDto? = null,
) {
    val refreshing: Boolean get() = loading && items != null
    val nodes: List<SharedItemDto> get() = items.orEmpty().filter { it.node != null }
    val albums: List<SharedItemDto> get() = items.orEmpty().filter { it.album != null }
}

/** A shared item's name: its folder, file or album. */
fun subjectName(item: SharedItemDto): String = item.node?.name ?: item.album?.name ?: ""

/** "Shared with me": folders, files and albums other people shared, and leaving them. */
class SharedViewModel(
    private val api: SquirrelApi,
    handoff: FileHandoff? = null,
    private val throttle: ResumeThrottle = ResumeThrottle(),
) : ViewModel() {
    private val _state = MutableStateFlow(SharedState())
    val state: StateFlow<SharedState> = _state.asStateFlow()

    /** A shared file that is not a photo opens with another app. */
    val opener = FileOpener(api, handoff, viewModelScope)

    init {
        load()
    }

    fun load() {
        if (_state.value.loading) return
        throttle.loaded()
        _state.update { it.copy(loading = true) }
        viewModelScope.launch {
            val result = call { api.sharing.sharedWithMe().bodyOrThrow() }
            _state.update { s ->
                when (result) {
                    is Outcome.Ok -> s.copy(items = result.value, loading = false, error = null)
                    is Outcome.Failed -> s.copy(loading = false, error = result.message)
                }
            }
        }
    }

    fun resumed() {
        if (throttle.due()) load()
    }

    fun askLeave(item: SharedItemDto) = _state.update { it.copy(leaving = item) }

    fun cancelLeave() = _state.update { it.copy(leaving = null) }

    fun confirmLeave() {
        val item = _state.value.leaving ?: return
        viewModelScope.launch {
            val result = call { api.sharing.deleteShare(item.shareId).okOrThrow() }
            _state.update { s ->
                when (result) {
                    is Outcome.Ok -> s.copy(leaving = null, items = s.items.orEmpty().filterNot { it.shareId == item.shareId })
                    is Outcome.Failed -> s.copy(leaving = null, error = result.message)
                }
            }
        }
    }
}
