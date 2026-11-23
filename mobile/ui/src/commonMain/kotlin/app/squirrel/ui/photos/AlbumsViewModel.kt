package app.squirrel.ui.photos

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.AccessLevel
import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.AssetSummary
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.api.models.TimelinePage
import app.squirrel.ui.ResumeThrottle
import app.squirrel.ui.attempt
import app.squirrel.ui.sharing.Abilities
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class AlbumsState(val albums: List<AlbumDto>? = null, val loading: Boolean = false, val error: String? = null) {
    /** Loading again over albums already shown (they stay until the new list arrives). */
    val refreshing: Boolean get() = loading && albums != null
}

/** The read-only album list. */
class AlbumsViewModel(private val api: SquirrelApi, private val throttle: ResumeThrottle = ResumeThrottle()) : ViewModel() {
    private val _state = MutableStateFlow(AlbumsState())
    val state: StateFlow<AlbumsState> = _state.asStateFlow()

    init {
        load()
    }

    fun load() {
        // `loading`, not the job: the load clears it in `finally`, while its job is still active.
        if (_state.value.loading) return
        throttle.loaded()
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

    /** The list again; the albums shown stay until it arrives. */
    fun refresh() = load()

    /** The app came back: a refresh, unless the list was loaded moments ago. */
    fun resumed() {
        if (throttle.due()) load()
    }
}

data class AlbumState(
    /** The album itself (name, count, access), once it arrived. */
    val album: AlbumDto? = null,
    val items: List<AssetSummary> = emptyList(),
    val nextCursor: String? = null,
    val started: Boolean = false,
    val loading: Boolean = false,
    /** Reloading from the first page: the photos stay until it replaces them. */
    val refreshing: Boolean = false,
    val error: String? = null,
) {
    val complete: Boolean get() = started && nextCursor == null

    /** Until the album says otherwise, nothing beyond looking is offered. */
    val can: Abilities get() = Abilities.of(album?.access ?: AccessLevel.read)
}

/** One album's photos, page by page (`GET /api/albums/{id}/items`). */
class AlbumViewModel(
    private val api: SquirrelApi,
    private val albumId: String,
    private val pageSize: Int = 120,
    private val throttle: ResumeThrottle = ResumeThrottle(),
) : ViewModel() {
    private val _state = MutableStateFlow(AlbumState())
    val state: StateFlow<AlbumState> = _state.asStateFlow()

    /** The page or refresh in flight; a refresh cancels a page. */
    private var loadJob: Job? = null

    /** Bumped on every refresh: a page asked for before it is dropped. */
    private var generation = 0

    /** The last refresh failed: "Try again" refreshes again (rather than paging on). */
    private var refreshFailed = false

    init {
        more()
        details()
    }

    /** The album's name, count, cover and access. */
    fun details() {
        viewModelScope.launch {
            attempt { api.albums.getAlbum(albumId).bodyOrThrow() }?.let { album -> _state.update { it.copy(album = album) } }
        }
    }

    /** The next page, when the end of the loaded rows comes into view. */
    fun more() {
        val current = _state.value
        // `loading`, not the job: a load clears it in `finally`, while its job is still active.
        if (current.loading || current.complete) return
        val gen = generation
        _state.update { it.copy(loading = true, error = null) }
        loadJob = viewModelScope.launch {
            try {
                val page = attempt { api.albums.listAlbumItems(albumId, cursor = current.nextCursor, limit = pageSize).bodyOrThrow() }
                update(gen) {
                    if (page == null) it.copy(error = UNREACHABLE)
                    else it.copy(items = (it.items + page.items).distinctBy { item -> item.id }, nextCursor = page.nextCursor, started = true)
                }
            } finally {
                update(gen) { it.copy(loading = false) }
            }
        }
    }

    /**
     * The album again from its first page. The photos stay until that page arrives; a page in
     * flight is cancelled and its answer dropped, so the two never mix.
     */
    fun refresh() {
        val current = _state.value
        if (current.refreshing) return
        if (!current.started) {
            if (current.loading) return // the first page is on its way
            return more()
        }
        loadJob?.cancel()
        val gen = ++generation
        throttle.loaded()
        details()
        _state.update { it.copy(loading = true, refreshing = true, error = null) }
        loadJob = viewModelScope.launch {
            var page: TimelinePage? = null
            try {
                page = attempt { api.albums.listAlbumItems(albumId, cursor = null, limit = pageSize).bodyOrThrow() }
            } finally {
                val fresh = page
                if (gen == generation) refreshFailed = fresh == null
                update(gen) { state ->
                    val settled = state.copy(loading = false, refreshing = false)
                    if (fresh == null) settled.copy(error = UNREACHABLE)
                    else settled.copy(items = fresh.items.distinctBy { it.id }, nextCursor = fresh.nextCursor)
                }
            }
        }
    }

    /** The app came back: a refresh, unless the album was loaded moments ago. */
    fun resumed() {
        if (_state.value.started && throttle.due()) refresh()
    }

    /** "Try again": the refresh that failed, or the page that did. */
    fun retry() = if (refreshFailed) refresh() else more()

    /** Applies [change] only while no refresh has started since [gen]. */
    private fun update(gen: Int, change: (AlbumState) -> AlbumState) {
        _state.update { if (gen == generation) change(it) else it }
    }

    /** The viewer shows [id]: the next page when it is close to the end of what is loaded. */
    fun near(id: String, margin: Int = VIEWER_MARGIN) {
        if (_state.value.error != null) return // "Try again" in the album is the way back
        val items = _state.value.items
        val at = items.indexOfFirst { it.id == id }
        if (at >= 0 && items.lastIndex - at < margin) more()
    }
}
