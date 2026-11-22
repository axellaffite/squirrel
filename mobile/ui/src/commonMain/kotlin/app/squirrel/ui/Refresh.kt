package app.squirrel.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.PullToRefresh
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.TimeSource

/** Coming back to the app reloads a list at most this often. */
val RESUME_REFRESH_INTERVAL: Duration = 30.seconds

/**
 * Whether a list is stale enough to reload when the app comes back. Created with the first
 * load, and every load since ([loaded]) counts: a resume right after one does nothing.
 */
class ResumeThrottle(
    private val interval: Duration = RESUME_REFRESH_INTERVAL,
    private val now: () -> Duration = monotonic(),
) {
    private var last: Duration = now()

    fun loaded() {
        last = now()
    }

    /** True (and counted as a load) when the last one is at least [interval] old. */
    fun due(): Boolean {
        if (now() - last < interval) return false
        loaded()
        return true
    }
}

private fun monotonic(): () -> Duration {
    val start = TimeSource.Monotonic.markNow()
    return { start.elapsedNow() }
}

/** Calls [onForeground] each time the screen is started: the app comes back, or the screen opens. */
@Composable
fun OnForeground(onForeground: () -> Unit) {
    val current by rememberUpdatedState(onForeground)
    LifecycleEventEffect(Lifecycle.Event.ON_START) { current() }
}

/**
 * Pull-to-refresh around content that does not scroll by itself (an empty state, an error):
 * a full-height scrollable column, so the pull reaches it like it reaches a list.
 */
@Composable
fun PullableColumn(
    refreshing: Boolean,
    onRefresh: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    PullToRefresh(refreshing, onRefresh) {
        Column(
            modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(Tokens.space2),
            content = content,
        )
    }
}
