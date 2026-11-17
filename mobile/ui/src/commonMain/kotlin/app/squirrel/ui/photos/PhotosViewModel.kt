package app.squirrel.ui.photos

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.shared.api.SignedOut
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

const val UNREACHABLE = "Your Squirrel could not be reached. Check your connection and try again."

/** The Photos timeline: months first, then each month's photos page by page as it scrolls in. */
class PhotosViewModel(private val api: SquirrelApi, private val pageSize: Int = 120) : ViewModel() {
    private val _state = MutableStateFlow(PhotosState())
    val state: StateFlow<PhotosState> = _state.asStateFlow()

    init {
        loadMonths()
    }

    private fun loadMonths() {
        _state.update { it.copy(loadingMonths = true, error = null) }
        viewModelScope.launch {
            val result = attempt { api.photos.photoMonths().bodyOrThrow() }
            _state.update { state ->
                if (result == null) state.copy(loadingMonths = false, error = UNREACHABLE)
                else state.copy(loadingMonths = false, months = result.map { MonthPage(it.month, it.count) })
            }
        }
    }

    /** The month scrolled into view: its next page, unless loading, complete or failed. */
    fun ensure(month: String) {
        var start = false
        var cursor: String? = null
        _state.update { state ->
            val page = state.months.firstOrNull { it.month == month } ?: return@update state
            if (page.loading || page.complete || page.failed) return@update state
            start = true
            cursor = page.nextCursor
            state.copy(months = state.months.map { if (it.month == month) it.copy(loading = true) else it })
        }
        if (!start) return
        viewModelScope.launch {
            val result = attempt { api.photos.listPhotos(month = month, cursor = cursor, limit = pageSize).bodyOrThrow() }
            _state.update { state ->
                state.copy(
                    error = if (result == null) UNREACHABLE else state.error,
                    months = state.months.map { page ->
                        when {
                            page.month != month -> page
                            result == null -> page.copy(loading = false, failed = true)
                            else -> page.copy(
                                items = page.items + result.items,
                                nextCursor = result.nextCursor,
                                started = true,
                                loading = false,
                            )
                        }
                    },
                )
            }
        }
    }

    /** After an error: months again if they failed, otherwise unblock failed months. */
    fun retry() {
        if (_state.value.months.isEmpty()) return loadMonths()
        _state.update { state -> state.copy(error = null, months = state.months.map { it.copy(failed = false) }) }
    }

    /** Null on failure (a refused device is signed out by the session; the app leaves the screen). */
    private suspend fun <T> attempt(call: suspend () -> T): T? = try {
        call()
    } catch (e: CancellationException) {
        throw e
    } catch (_: SignedOut) {
        null
    } catch (_: Exception) {
        null
    }
}
