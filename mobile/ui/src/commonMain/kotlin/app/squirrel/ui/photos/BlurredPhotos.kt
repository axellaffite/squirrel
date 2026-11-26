package app.squirrel.ui.photos

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TimeSource

/** Refreshes closer together than this are skipped (coming back to the app often). */
val BLUR_REFRESH_INTERVAL: Duration = 10.seconds

/**
 * The app-wide blurred photos (see [BlurState]): loaded once signed in ([start]), refreshed when
 * the app comes back and after the filter changes. Every thumbnail, cover and preview asks
 * [BlurState.isBlurred] before loading a photo. A failed load shows photos normally.
 */
class BlurredPhotos(
    private val scope: CoroutineScope,
    /** Remembers, across launches, whether the filter blurred anything (for the first screen). */
    private val remember: suspend (active: Boolean) -> Unit,
    private val interval: Duration = BLUR_REFRESH_INTERVAL,
    private val now: () -> Duration = monotonicClock(),
    private val log: (String) -> Unit = ::println,
) {
    private val _state = MutableStateFlow(BlurState())
    val state: StateFlow<BlurState> = _state.asStateFlow()

    private var load: (suspend () -> List<String>)? = null
    private var last: Duration? = null
    private var job: Job? = null

    /**
     * Signed in: blurs every photo ahead when the filter blurred something last time
     * ([wasActive]), then asks the server with [load].
     */
    fun start(load: suspend () -> List<String>, wasActive: Boolean) {
        this.load = load
        job?.cancel()
        _state.value = BlurState(wasActive = wasActive)
        last = now()
        job = scope.launch { fetch(load) }
    }

    /** Asks the server again; [force] skips the throttle (the filter just changed). */
    fun refresh(force: Boolean = false) {
        val load = load ?: return
        val previous = last
        if (!force && previous != null && now() - previous < interval) return
        last = now()
        job?.cancel()
        job = scope.launch { fetch(load) }
    }

    fun reveal(id: String) = _state.update { it.withRevealed(id) }

    fun setRevealAll(all: Boolean) = _state.update { it.withRevealAll(all) }

    /** Signed out: nothing is known any more (the next account starts afresh). */
    fun reset() {
        job?.cancel()
        job = null
        load = null
        last = null
        _state.value = BlurState()
    }

    private suspend fun fetch(load: suspend () -> List<String>) {
        val ids = try {
            load()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            // Photos are shown rather than blurred forever; the next refresh tries again.
            log("Blurred photos could not be loaded: ${e.message}")
            _state.update { if (it.ids == null) it.withIds(emptyList()) else it }
            return
        }
        _state.update { it.withIds(ids) }
        try {
            remember(ids.isNotEmpty())
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            log("The blur filter's state could not be remembered: ${e.message}")
        }
    }
}

private fun monotonicClock(): () -> Duration {
    val start = TimeSource.Monotonic.markNow()
    return { start.elapsedNow() }
}

/** What thumbnails read: the blurred photos now, and how to reveal them. */
@Immutable
data class BlurView(
    val state: BlurState = BlurState(),
    val reveal: (String) -> Unit = {},
    val setRevealAll: (Boolean) -> Unit = {},
) {
    fun isBlurred(id: String?): Boolean = id != null && state.isBlurred(id)
}

/** The signed-in account's blurred photos (nothing blurred where not provided: previews, tests). */
val LocalBlur = compositionLocalOf { BlurView() }
