package app.squirrel.ui.shell

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf

/** The one full-screen layer above the shell (the photo viewer), drawn edge to edge. */
@Stable
class OverlayHost {
    internal var content by mutableStateOf<(@Composable () -> Unit)?>(null)

    /** Draws the layer; the app root calls this after the shell. */
    @Composable
    fun Layer() {
        content?.invoke()
    }
}

val LocalOverlay = staticCompositionLocalOf<OverlayHost?> { null }

/**
 * Shows [content] full screen, over the shell and behind the system bars, for as long as this
 * call stays in the composition. Without a host (previews) it draws in place.
 */
@Composable
fun FullScreen(content: @Composable () -> Unit) {
    val host = LocalOverlay.current ?: return content()
    val latest by rememberUpdatedState(content)
    DisposableEffect(host) {
        val slot: @Composable () -> Unit = { latest() }
        host.content = slot
        onDispose { if (host.content === slot) host.content = null }
    }
}
