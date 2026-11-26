package app.squirrel.ui.photos

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import app.squirrel.ui.testing.runReal
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeout
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

class BlurStateTest {
    @Test
    fun onlyListedPhotosAreBlurredOnceLoaded() {
        val state = BlurState().withIds(listOf("a", "b"))
        assertTrue(state.isBlurred("a"))
        assertFalse(state.isBlurred("c"))
        assertTrue(state.any)
        assertTrue(state.wasActive)
    }

    @Test
    fun beforeTheListEverythingIsBlurredOnlyIfTheFilterWasActive() {
        assertTrue(BlurState(wasActive = true).isBlurred("anything"))
        assertFalse(BlurState(wasActive = false).isBlurred("anything"))
        assertFalse(BlurState(wasActive = true).any, "nothing to show or blur again before the list")
    }

    @Test
    fun revealingOnePhotoLeavesTheOthersBlurred() {
        val state = BlurState().withIds(listOf("a", "b")).withRevealed("a")
        assertFalse(state.isBlurred("a"))
        assertTrue(state.isBlurred("b"))
    }

    @Test
    fun revealAllShowsEveryPhotoAndBlurAgainForgetsSingleReveals() {
        val shown = BlurState().withIds(listOf("a", "b")).withRevealed("a").withRevealAll(true)
        assertFalse(shown.isBlurred("a"))
        assertFalse(shown.isBlurred("b"))
        val again = shown.withRevealAll(false)
        assertTrue(again.isBlurred("a"), "blur again hides the one shown alone too")
        assertTrue(again.isBlurred("b"))
    }

    @Test
    fun anEmptyListBlursNothingAndIsRememberedAsInactive() {
        val state = BlurState(wasActive = true).withIds(emptyList())
        assertFalse(state.isBlurred("a"))
        assertFalse(state.any)
        assertFalse(state.wasActive)
    }

    @Test
    fun blurViewNeverBlursWithoutAnId() {
        val view = BlurView(BlurState(wasActive = true))
        assertFalse(view.isBlurred(null))
        assertTrue(view.isBlurred("a"))
    }
}

class BlurredPhotosTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val remembered = mutableListOf<Boolean>()
    private var clock = Duration.ZERO

    @AfterTest
    fun tearDown() = scope.cancel()

    private fun blurred() = BlurredPhotos(scope, remember = { remembered += it }, interval = 10.seconds, now = { clock }, log = {})

    private suspend fun BlurredPhotos.awaitLoaded(): BlurState = withTimeout(5_000) { state.first { it.ids != null } }

    @Test
    fun startBlursAheadThenUsesTheServerList() = runReal {
        val photos = blurred()
        photos.start(load = { listOf("a") }, wasActive = true)
        val state = photos.awaitLoaded()
        assertEquals(setOf("a"), state.ids)
        assertFalse(state.isBlurred("b"))
        withTimeout(5_000) { while (remembered.isEmpty()) delay(10) }
        assertEquals(listOf(true), remembered)
    }

    @Test
    fun aFailedLoadShowsPhotosNormally() = runReal {
        val photos = blurred()
        photos.start(load = { error("offline") }, wasActive = true)
        val state = photos.awaitLoaded()
        assertFalse(state.isBlurred("a"))
        assertTrue(remembered.isEmpty(), "a failure does not forget that the filter was active")
    }

    @Test
    fun refreshIsThrottledUnlessForced() = runReal {
        val photos = blurred()
        var calls = 0
        photos.start(load = { calls++; listOf("a") }, wasActive = false)
        photos.awaitLoaded()
        photos.refresh()
        assertEquals(1, calls, "too soon after the first load")
        photos.refresh(force = true)
        withTimeout(5_000) { while (calls < 2) delay(10) }
        clock += 11.seconds
        photos.refresh()
        withTimeout(5_000) { while (calls < 3) delay(10) }
    }

    @Test
    fun resetForgetsTheListAndTheReveals() = runReal {
        val photos = blurred()
        photos.start(load = { listOf("a") }, wasActive = false)
        photos.awaitLoaded()
        photos.setRevealAll(true)
        photos.reset()
        assertEquals(BlurState(), photos.state.value)
        photos.refresh(force = true)
        assertEquals(BlurState(), photos.state.value, "signed out: nothing to ask")
    }
}
