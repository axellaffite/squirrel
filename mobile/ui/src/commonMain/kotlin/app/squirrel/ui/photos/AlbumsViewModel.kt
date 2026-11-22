package app.squirrel.ui.photos

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.AssetSummary
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.attempt
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class AlbumsState(val albums: List<AlbumDto>? = null, val loading: Boolean = false, val error: String? = null)

/** The read-only album list. */
class AlbumsViewModel(private val api: SquirrelApi) : ViewModel() {
    private val _state = MutableStateFlow(AlbumsState())
    val state: StateFlow<AlbumsState> = _state.asStateFlow()

    init {
        load()
    }

    fun load() {
        // `loading`, not the job: the load clears it in `finally`, while its job is still active.
        if (_state.value.loading) return
        _state.update { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                val albums = attempt { api.albums.listAlbums().bodyOrThrow() }
                _state.update { if (albums == null) it.copy(error = UNREACHABLE) else it.copy(albums = albums) }
            } finally {
                _state.update { it.copy(loading = false) }
            }
        }
    }
}

data class AlbumState(
    val items: List<AssetSummary> = emptyList(),
    val nextCursor: String? = null,
    val started: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
) {
    val complete: Boolean get() = started && nextCursor == null
}

/** One album's photos, page by page (`GET /api/albums/{id}/items`). */
class AlbumViewModel(private val api: SquirrelApi, private val albumId: String, private val pageSize: Int = 120) : ViewModel() {
    private val _state = MutableStateFlow(AlbumState())
    val state: StateFlow<AlbumState> = _state.asStateFlow()

    init {
        more()
    }

    /** The next page, when the end of the loaded rows comes into view. */
    fun more() {
        val current = _state.value
        if (current.loading || current.complete) return
        _state.update { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                val page = attempt { api.albums.listAlbumItems(albumId, cursor = current.nextCursor, limit = pageSize).bodyOrThrow() }
                _state.update {
                    if (page == null) it.copy(error = UNREACHABLE)
                    else it.copy(items = (it.items + page.items).distinctBy { item -> item.id }, nextCursor = page.nextCursor, started = true)
                }
            } finally {
                _state.update { it.copy(loading = false) }
            }
        }
    }

    /** The viewer shows [id]: the next page when it is close to the end of what is loaded. */
    fun near(id: String, margin: Int = VIEWER_MARGIN) {
        if (_state.value.error != null) return // "Try again" in the album is the way back
        val items = _state.value.items
        val at = items.indexOfFirst { it.id == id }
        if (at >= 0 && items.lastIndex - at < margin) more()
    }
}
