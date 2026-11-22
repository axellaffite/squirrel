package app.squirrel.ui.photos

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.MonthCountDto
import app.squirrel.api.models.TimelinePage
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.ResumeThrottle
import app.squirrel.ui.attempt
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** How close to the end of what is loaded the viewer asks for more. */
const val VIEWER_MARGIN = 10

const val UNREACHABLE = "Your Squirrel could not be reached. Check your connection and try again."

/** The Photos timeline: months first, then each month's photos page by page as it scrolls in. */
class PhotosViewModel(
    private val api: SquirrelApi,
    private val pageSize: Int = 120,
    private val throttle: ResumeThrottle = ResumeThrottle(),
) : ViewModel() {
    private val _state = MutableStateFlow(PhotosState())
    val state: StateFlow<PhotosState> = _state.asStateFlow()

    /** The last refresh failed: "Try again" refreshes again. */
    private var refreshFailed = false

    init {
        loadMonths()
    }

    private fun loadMonths() {
        throttle.loaded()
        _state.update { it.copy(loadingMonths = true, error = null) }
        viewModelScope.launch {
            try {
                val result = attempt { api.photos.photoMonths().bodyOrThrow() }
                _state.update { state ->
                    if (result == null) state.copy(error = UNREACHABLE)
                    else state.copy(months = result.map { MonthPage(it.month, it.count) })
                }
            } finally {
                _state.update { it.copy(loadingMonths = false) }
            }
        }
    }

    /** The month scrolled into view: its next page, unless loading, complete or failed. */
    fun ensure(month: String) {
        var start = false
        var cursor: String? = null
        var epoch = 0
        _state.update { state ->
            val page = state.months.firstOrNull { it.month == month } ?: return@update state
            if (page.loading || page.complete || page.failed) return@update state
            start = true
            cursor = page.nextCursor
            epoch = page.epoch
            state.copy(months = state.months.map { if (it.month == month) it.copy(loading = true) else it })
        }
        if (start) fetch(month, cursor, epoch)
    }

    /** One page of [month], already marked loading. */
    private fun fetch(month: String, cursor: String?, epoch: Int) {
        viewModelScope.launch {
            var result: TimelinePage? = null
            try {
                result = attempt { api.photos.listPhotos(month = month, cursor = cursor, limit = pageSize).bodyOrThrow() }
            } finally {
                settle(month, epoch, result)
            }
        }
    }

    /** Ends a month's page load: its items, or a failure the "Try again" button can clear. */
    private fun settle(month: String, epoch: Int, result: TimelinePage?) {
        _state.update { state ->
            // A refresh replaced the month meanwhile: this page belongs to the old one.
            if (state.months.none { it.month == month && it.epoch == epoch }) return@update state
            state.copy(
                error = if (result == null) UNREACHABLE else state.error,
                months = state.months.map { page ->
                    when {
                        page.month != month -> page
                        result == null -> page.copy(loading = false, failed = true)
                        else -> page.copy(
                            items = (page.items + result.items).distinctBy { it.id },
                            nextCursor = result.nextCursor,
                            started = true,
                            loading = false,
                        )
                    }
                },
            )
        }
    }

    /**
     * The viewer shows [id]: within [margin] of its month's loaded end, that month's next page;
     * at the end of a complete month, the next month, so paging carries on across months.
     */
    fun near(id: String, margin: Int = VIEWER_MARGIN) {
        val months = _state.value.months
        val at = months.indexOfFirst { page -> page.items.any { it.id == id } }
        if (at < 0) return
        val page = months[at]
        val left = page.items.lastIndex - page.items.indexOfFirst { it.id == id }
        if (left >= margin) return
        if (!page.complete) return ensure(page.month)
        months.getOrNull(at + 1)?.takeIf { !it.started }?.let { ensure(it.month) }
    }

    /**
     * After an error: months again if they failed, otherwise the months that failed load again
     * (they were on screen when they failed; their placeholders' effects will not run twice).
     */
    fun retry() {
        val current = _state.value
        if (current.loadingMonths || current.refreshing) return // its answer is on the way
        if (current.months.isEmpty()) return loadMonths()
        if (refreshFailed) return refresh()
        val failed = _state.value.months.filter { it.failed }.map { it.month }
        _state.update { state -> state.copy(error = null, months = state.months.map { it.copy(failed = false) }) }
        failed.forEach(::ensure)
    }

    /**
     * The timeline again from the top. Months come first; a month whose count changed and had
     * photos showing gets its first page again, and the swap happens only once every such page
     * has arrived, so nothing blanks meanwhile. Unchanged months keep what they had loaded.
     */
    fun refresh() {
        var start = false
        _state.update { state ->
            start = !state.loadingMonths && !state.refreshing
            if (start) state.copy(refreshing = true) else state
        }
        if (!start) return
        throttle.loaded()
        viewModelScope.launch {
            var fresh: List<MonthPage>? = null
            try {
                fresh = reloaded()
            } finally {
                apply(fresh)
            }
        }
    }

    /** The app came back: a refresh, unless the timeline was loaded moments ago. */
    fun resumed() {
        if (!_state.value.loadingMonths && throttle.due()) refresh()
    }

    /** The new months, with first pages for the changed ones that were showing photos; null: failed. */
    private suspend fun reloaded(): List<MonthPage>? {
        val months = attempt { api.photos.photoMonths().bodyOrThrow() } ?: return null
        val before = _state.value.months.associateBy { it.month }
        val stale = months.filter { m -> before[m.month]?.let { it.started && it.count != m.count } == true }
        val firsts = coroutineScope {
            stale.map { m -> async { m.month to attempt { api.photos.listPhotos(month = m.month, cursor = null, limit = pageSize).bodyOrThrow() } } }.awaitAll()
        }.toMap()
        if (firsts.values.any { it == null }) return null
        return months.map { m -> fresh(m, firsts[m.month]) }
    }

    private fun fresh(month: MonthCountDto, first: TimelinePage?): MonthPage =
        if (first == null) MonthPage(month.month, month.count)
        else MonthPage(month.month, month.count, first.items.distinctBy { it.id }, first.nextCursor, started = true)

    /** Swaps the refreshed months in; months whose count did not change keep their pages. */
    private fun apply(fresh: List<MonthPage>?) {
        refreshFailed = fresh == null
        val again = mutableListOf<MonthPage>()
        _state.update { state ->
            again.clear() // `update` may run this more than once
            if (fresh == null) return@update state.copy(refreshing = false, error = UNREACHABLE)
            val now = state.months.associateBy { it.month }
            val months = fresh.map { next ->
                val old = now[next.month] ?: return@map next
                when {
                    next.started -> next.copy(epoch = old.epoch + 1)
                    old.count == next.count -> if (old.failed) old.copy(failed = false) else old
                    else -> next.copy(epoch = old.epoch + 1)
                }.let { kept ->
                    // On screen before (asked for, or failed): its placeholders' effects will not run
                    // again, so its next page starts here, in the same update (never seen idle).
                    if ((old.loading || old.failed) && !kept.loading && !kept.complete) kept.copy(loading = true).also { again += it }
                    else kept
                }
            }
            state.copy(months = months, refreshing = false, error = null)
        }
        again.forEach { fetch(it.month, it.nextCursor, it.epoch) }
    }
}
