package app.squirrel.ui.search

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.PersonDto
import app.squirrel.api.models.PhotoSearchResults
import app.squirrel.api.models.SearchOrder
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.attempt
import app.squirrel.ui.explore.PhotoFilter
import app.squirrel.ui.photos.UNREACHABLE
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class PhotoSearchState(
    /** What is in the field. */
    val typed: String = "",
    /** The query the results belong to ('' before any search). */
    val query: String = "",
    val filter: PhotoFilter = PhotoFilter(),
    /** The order asked for (the next search's). */
    val order: SearchOrder = SearchOrder.date,
    val items: List<AssetSummary> = emptyList(),
    val hasMore: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
    /** Search is not set up on this server (no text model): nothing to retry. */
    val unavailable: Boolean = false,
    /** The caller's people, to name and show a person filter. */
    val people: List<PersonDto> = emptyList(),
) {
    /** The results belong to what is typed and nothing is loading: "No photos match" may show. */
    val settled: Boolean get() = query.isNotEmpty() && query == normalizeQuery(typed) && !loading
}

/** One page of a search: relevance pages by offset, date by the previous page's cursor. */
private data class Page(val order: SearchOrder, val offset: Int = 0, val cursor: String? = null) {
    val first: Boolean get() = if (order == SearchOrder.date) cursor == null else offset == 0
}

/**
 * As-you-type photo search: waits for a pause in typing, cancels the request a newer query (or
 * filter, or order) replaced, pages as the grid nears its end, retries while the text model
 * starts. Only the latest search's answers are ever shown.
 */
class PhotoSearchViewModel(
    private val api: SquirrelApi,
    private val debounceMs: Long = SEARCH_DEBOUNCE_MS,
    private val busyRetryMs: Long = BUSY_RETRY_MS,
) : ViewModel() {
    private val _state = MutableStateFlow(PhotoSearchState())
    val state: StateFlow<PhotoSearchState> = _state.asStateFlow()

    private var job: Job? = null

    /** Where the next date-ordered page starts. */
    private var cursor: String? = null

    init {
        viewModelScope.launch {
            // Starts the server's text model, so the first search is quick; a failure says nothing.
            attempt { api.search.warmUpSearch() }
            attempt { api.people.listPeople().bodyOrThrow() }?.let { people -> _state.update { it.copy(people = people) } }
        }
    }

    /** A new query typed: searches after the pause, or clears for an empty one. */
    fun type(raw: String) {
        _state.update { it.copy(typed = raw) }
        val query = normalizeQuery(raw)
        if (query == _state.value.query && query.isNotEmpty() && !_state.value.loading) return cancelPending()
        if (!isPhotoQuery(query)) {
            cancelPending()
            return clear(if (query.length > MAX_PHOTO_QUERY) PHOTO_TOO_LONG else null)
        }
        start(query, firstPage(), debounceMs)
    }

    /** Searches at once (the keyboard's search key). */
    fun now() {
        val query = normalizeQuery(_state.value.typed)
        if (isPhotoQuery(query)) start(query, firstPage(), 0)
    }

    /** Narrows the results: searches the current query again at once. */
    fun setFilter(filter: PhotoFilter) {
        val next = filter.normalized()
        if (next == _state.value.filter) return
        _state.update { it.copy(filter = next) }
        researchNow()
    }

    /** Orders the results otherwise: searches the current query again at once. */
    fun setOrder(order: SearchOrder) {
        if (order == _state.value.order) return
        _state.update { it.copy(order = order) }
        researchNow()
    }

    /** The next page, when the grid nears its end. */
    fun more() {
        val s = _state.value
        if (!s.hasMore || s.loading || s.items.size >= MAX_PHOTO_RESULTS) return
        val page = if (s.order == SearchOrder.relevance) Page(SearchOrder.relevance, offset = s.items.size) else Page(SearchOrder.date, cursor = cursor ?: return)
        start(s.query, page, 0)
    }

    /** "Try again" after an error. */
    fun retry() {
        val s = _state.value
        if (s.items.isNotEmpty() && s.hasMore) more() else now()
    }

    private fun researchNow() {
        val query = normalizeQuery(_state.value.typed)
        if (isPhotoQuery(query)) start(query, firstPage(), 0) else clear(null)
    }

    private fun firstPage() = Page(_state.value.order)

    private fun cancelPending() {
        job?.cancel()
        job = null
        _state.update { it.copy(loading = false) }
    }

    private fun clear(error: String?) {
        cursor = null
        _state.update { it.copy(query = "", items = emptyList(), hasMore = false, error = error, loading = false) }
    }

    private fun start(query: String, page: Page, wait: Long) {
        job?.cancel()
        _state.update { it.copy(loading = true) }
        job = viewModelScope.launch {
            if (wait > 0) delay(wait)
            fetch(query, page)
        }
    }

    private suspend fun fetch(query: String, page: Page) {
        var busyTries = 0
        var retried = false
        while (true) {
            val outcome = request(query, page)
            val error = outcome.exceptionOrNull()
            if (error == null) return apply(query, page, outcome.getOrThrow())
            val api = error as? ApiError
            when {
                api?.status == 503 && api.code == "ml_unavailable" -> return fail(page, SEARCH_UNAVAILABLE, unavailable = true)
                api?.status == 503 && busyTries < BUSY_RETRIES -> {
                    busyTries += 1
                    _state.update { it.copy(error = null) }
                    delay(busyRetryMs)
                }
                api?.status == 503 -> return fail(page, SEARCH_BUSY)
                api?.status == 429 && !retried && retryWait(api) != null -> {
                    retried = true
                    _state.update { it.copy(error = SEARCH_TOO_FAST) }
                    delay(retryWait(api) ?: RETRY_AFTER_MS)
                }
                api?.status == 429 -> return fail(page, STILL_TOO_FAST)
                api != null && api.fromServer -> return fail(page, api.message ?: UNREACHABLE)
                else -> return fail(page, UNREACHABLE)
            }
        }
    }

    private suspend fun request(query: String, page: Page): Result<PhotoSearchResults> = try {
        val f = _state.value.filter
        val limit = if (page.order == SearchOrder.date) PHOTO_DATE_PAGE else PHOTO_PAGE.coerceAtMost(MAX_PHOTO_RESULTS - page.offset).coerceAtLeast(1)
        Result.success(
            api.search.searchPhotos(
                q = query,
                order = page.order,
                offset = if (page.order == SearchOrder.relevance) page.offset else null,
                cursor = page.cursor,
                limit = limit,
                person = f.person,
                tag = f.tag,
                country = f.country,
                region = f.region,
                city = f.city,
                cameraMake = f.cameraMake,
                cameraModel = f.cameraModel,
                kind = f.kind,
            ).bodyOrThrow(),
        )
    } catch (e: CancellationException) {
        // A library cancelling internally, while this search is still wanted, is a failure.
        if (!currentCoroutineContext().isActive) throw e
        Result.failure(e)
    } catch (e: Exception) {
        Result.failure(e)
    }

    private fun apply(query: String, page: Page, result: PhotoSearchResults) {
        cursor = if (page.order == SearchOrder.date) result.nextCursor else null
        _state.update { s ->
            val items = if (page.first) result.items else (s.items + result.items).distinctBy { it.id }
            val reachable = page.order == SearchOrder.relevance || cursor != null
            s.copy(
                query = query,
                items = items,
                hasMore = result.hasMore && reachable && items.size < MAX_PHOTO_RESULTS,
                loading = false,
                error = null,
                unavailable = false,
            )
        }
    }

    private fun fail(page: Page, message: String, unavailable: Boolean = false) {
        if (page.first) cursor = null
        _state.update { s ->
            // Results of another search must not pass for this one's.
            val base = if (page.first) s.copy(query = "", items = emptyList(), hasMore = false) else s
            base.copy(loading = false, error = message, unavailable = unavailable)
        }
    }
}
