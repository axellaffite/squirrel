package app.squirrel.theme.components

import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Velocity
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.cardShadow
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/**
 * The pull gesture without Compose around it: [distance] follows the finger at [RESISTANCE] of
 * its travel, up to [MAX_PULL] × [threshold]; letting go at or past [threshold] refreshes.
 */
class PullTracker(val threshold: Float) {
    var distance by mutableFloatStateOf(0f)
        private set

    /** A finger is pulling the indicator down (as opposed to it settling on its own). */
    var pulling = false
        private set

    val fraction: Float get() = if (threshold <= 0f) 0f else (distance / threshold).coerceIn(0f, 1f)
    val armed: Boolean get() = distance >= threshold

    /** What the list could not scroll at its top ([delta] > 0): all of it moves the indicator. */
    fun pull(delta: Float): Float {
        if (delta <= 0f) return 0f
        pulling = true
        distance = (distance + delta * RESISTANCE).coerceAtMost(threshold * MAX_PULL)
        return delta
    }

    /** Scrolling back up while pulled ([delta] < 0): taken until the indicator is back. */
    fun push(delta: Float): Float {
        if (delta >= 0f || distance <= 0f) return 0f
        val next = (distance + delta * RESISTANCE).coerceAtLeast(0f)
        val consumed = (next - distance) / RESISTANCE
        distance = next
        if (next == 0f) pulling = false
        return consumed
    }

    /** The finger lifts: true when it was pulled far enough. The indicator then settles. */
    fun release(): Boolean {
        val refresh = pulling && armed
        pulling = false
        return refresh
    }

    /** Settling (released, or a refresh starting or ending elsewhere). */
    fun settle(value: Float) {
        if (!pulling) distance = value
    }

    companion object {
        /** The indicator moves half as far as the finger: it reads as a pull, not a scroll. */
        const val RESISTANCE = 0.5f

        /** How far past the threshold it can be pulled. */
        const val MAX_PULL = 1.5f
    }
}

/**
 * Pull-to-refresh over a scrollable [content] (a lazy list, or a `verticalScroll` column for
 * empty and error states). The content never moves or blanks: a white round well with the
 * accent arcs comes down from the top, follows the finger, and stays while [refreshing].
 * Motion is MASTER.md's: 240 ms on the one ease, instant with reduced motion, no overshoot.
 * Assistive tech gets a "[label]" action, since the gesture itself is not reachable there.
 */
@Composable
fun PullToRefresh(
    refreshing: Boolean,
    onRefresh: () -> Unit,
    modifier: Modifier = Modifier,
    label: String = "Refresh",
    content: @Composable BoxScope.() -> Unit,
) {
    val threshold = with(LocalDensity.current) { Tokens.space8.toPx() }
    val tracker = remember(threshold) { PullTracker(threshold) }
    val motion by rememberUpdatedState(SquirrelTheme.motion)
    val scope = rememberCoroutineScope()
    val refresh by rememberUpdatedState(onRefresh)
    val busy by rememberUpdatedState(refreshing)
    val settle = remember(tracker) { SettleRunner(tracker) }

    LaunchedEffect(refreshing, tracker) {
        settle.to(if (refreshing) threshold else 0f, motion.base())
    }
    val connection = remember(tracker) {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset =
                if (source == NestedScrollSource.UserInput && tracker.pulling) Offset(0f, tracker.push(available.y)) else Offset.Zero

            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (source != NestedScrollSource.UserInput || busy) return Offset.Zero
                settle.stop()
                return Offset(0f, tracker.pull(available.y))
            }

            override suspend fun onPreFling(available: Velocity): Velocity {
                if (!tracker.pulling) return Velocity.Zero
                val go = tracker.release()
                if (go) refresh()
                scope.launch { settle.to(if (go || busy) threshold else 0f, motion.base()) }
                return available // the list does not fling away under the indicator
            }
        }
    }
    Box(
        modifier = modifier
            .fillMaxSize()
            .clipToBounds()
            .nestedScroll(connection)
            .semantics { customActions = listOf(CustomAccessibilityAction(label) { refresh(); true }) },
    ) {
        content()
        RefreshIndicator(tracker, refreshing, Modifier.align(Alignment.TopCenter))
    }
}

/** Runs one settling animation at a time; a new pull stops it. */
private class SettleRunner(private val tracker: PullTracker) {
    private var job: Job? = null

    suspend fun to(target: Float, spec: FiniteAnimationSpec<Float>) {
        if (tracker.pulling) return
        kotlinx.coroutines.coroutineScope {
            job?.cancel()
            job = launch {
                animate(tracker.distance, target, animationSpec = spec) { value, _ -> tracker.settle(value) }
            }
        }
    }

    fun stop() {
        job?.cancel()
    }
}

/** Two accent arcs in a white well: they grow with the pull, then turn while refreshing. */
@Composable
private fun RefreshIndicator(tracker: PullTracker, refreshing: Boolean, modifier: Modifier) {
    val scheme = SquirrelTheme.scheme
    val motion = SquirrelTheme.motion
    val well = with(LocalDensity.current) { Tokens.wellMd.toPx() }
    val stroke = with(LocalDensity.current) { Tokens.ringWidth.toPx() }
    val turn = if (refreshing && !motion.reduced) {
        // Half a turn per cycle: the two arcs are symmetric, so the restart is seamless.
        val transition = rememberInfiniteTransition()
        transition.animateFloat(
            initialValue = 0f,
            targetValue = HALF_TURN,
            animationSpec = infiniteRepeatable(tween(motion.slowMillis, easing = Tokens.ease), RepeatMode.Restart),
        )
    } else {
        null
    }
    Box(
        modifier = modifier
            .graphicsLayer {
                val shown = tracker.distance
                translationY = shown - well
                alpha = if (refreshing) 1f else tracker.fraction
            }
            .padding(top = Tokens.space1)
            .size(Tokens.wellMd)
            .cardShadow(Shapes.circle)
            .background(scheme.card, Shapes.circle)
            .then(
                if (refreshing) {
                    Modifier.semantics {
                        contentDescription = "Refreshing"
                        liveRegion = LiveRegionMode.Polite
                        progressBarRangeInfo = ProgressBarRangeInfo.Indeterminate
                    }
                } else {
                    Modifier
                },
            ),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.size(Tokens.iconSm)) {
            val grown = if (refreshing) 1f else tracker.fraction
            val sweep = ARC * grown
            val start = (turn?.value ?: 0f) + HALF_TURN * tracker.fraction
            val style = Stroke(width = stroke, cap = StrokeCap.Round)
            if (sweep > 0f) {
                drawArc(scheme.accent, start, sweep, useCenter = false, style = style)
                drawArc(scheme.accent, start + HALF_TURN, sweep, useCenter = false, style = style)
            }
        }
    }
}

private const val HALF_TURN = 180f
private const val ARC = 120f
