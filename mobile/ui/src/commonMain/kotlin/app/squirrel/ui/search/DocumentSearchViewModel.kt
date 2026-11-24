package app.squirrel.ui.search

import app.squirrel.ui.mapState
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.DocumentHit
import app.squirrel.api.models.DocumentResults
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.FileOpener
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

data class DocumentSearchState(
    val typed: String = "",
    /** The query the results belong to ('' before any search). */
    val query: String = "",
    val items: List<DocumentHit> = emptyList(),
    val hasMore: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
) {
    val settled: Boolean get() = query.isNotEmpty() && query == normalizeQuery(typed) && !loading
    val hint: String? get() = if (isDocumentQuery(typed)) shortWordHint(typed) else null
}

/**
 * Document search (`/api/search/documents`): file names, PDFs and text files by their content.
 * As-you-type with a pause, "Load more" by offset, one retry after a 429.
 */
class DocumentSearchViewModel(
    private val api: SquirrelApi,
    handoff: FileHandoff?,
    private val debounceMs: Long = SEARCH_DEBOUNCE_MS,
) : ViewModel() {
    private val _state = MutableStateFlow(DocumentSearchState())
    val state: StateFlow<DocumentSearchState> = _state.asStateFlow()

    /** The search as LS-DO shows it (see [DocumentSearchLsDo]); one flow for the session. */
    val listing: StateFlow<DocumentSearchLsDoState> = state.mapState(::documentSearchLsDoState)

    /** Opening a result with another app. */
    val opener = FileOpener(api, handoff, viewModelScope)

    private var job: Job? = null

    fun type(raw: String) {
        _state.update { it.copy(typed = raw) }
        val query = normalizeQuery(raw)
        if (query == _state.value.query && query.isNotEmpty() && !_state.value.loading) return cancel()
        if (!isDocumentQuery(query)) {
            cancel()
            return _state.update { it.copy(query = "", items = emptyList(), hasMore = false, error = null) }
        }
        start(query, 0, debounceMs)
    }

    fun now() {
        val query = normalizeQuery(_state.value.typed)
        if (isDocumentQuery(query)) start(query, 0, 0)
    }

    fun more() {
        val s = _state.value
        if (!s.hasMore || s.loading || s.items.size >= MAX_DOC_RESULTS) return
        start(s.query, s.items.size, 0)
    }

    fun retry() = if (_state.value.items.isNotEmpty()) more() else now()

    private fun cancel() {
        job?.cancel()
        job = null
        _state.update { it.copy(loading = false) }
    }

    private fun start(query: String, offset: Int, wait: Long) {
        job?.cancel()
        _state.update { it.copy(loading = true) }
        job = viewModelScope.launch {
            if (wait > 0) delay(wait)
            fetch(query, offset)
        }
    }

    private suspend fun fetch(query: String, offset: Int) {
        var retried = false
        while (true) {
            val outcome = request(query, offset)
            val error = outcome.exceptionOrNull()
            if (error == null) return apply(query, offset, outcome.getOrThrow())
            val api = error as? ApiError
            val wait = api?.takeIf { it.status == 429 }?.let(::retryWait)
            if (wait != null && !retried) {
                retried = true
                _state.update { it.copy(error = SEARCH_TOO_FAST) }
                delay(wait)
                continue
            }
            val message = if (api?.status == 429) STILL_TOO_FAST else documentSearchError(api)
            return _state.update { s ->
                // Results of an older query must not pass for this one's.
                val base = if (offset == 0 && query != s.query) s.copy(query = "", items = emptyList(), hasMore = false) else s
                base.copy(loading = false, error = message)
            }
        }
    }

    private suspend fun request(query: String, offset: Int): Result<DocumentResults> = try {
        val limit = DOC_PAGE.coerceAtMost(MAX_DOC_RESULTS - offset)
        Result.success(api.search.searchDocuments(q = query, offset = offset, limit = limit).bodyOrThrow())
    } catch (e: CancellationException) {
        if (!currentCoroutineContext().isActive) throw e
        Result.failure(e)
    } catch (e: Exception) {
        Result.failure(e)
    }

    private fun apply(query: String, offset: Int, result: DocumentResults) = _state.update { s ->
        val items = if (offset == 0) result.items else (s.items + result.items).distinctBy { it.node.id }
        s.copy(query = query, items = items, hasMore = result.hasMore && items.size < MAX_DOC_RESULTS, loading = false, error = null)
    }
}
