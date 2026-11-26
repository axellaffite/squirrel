package app.squirrel.theme

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur

/**
 * Whether this platform blurs content drawn under a modal (`Modifier.blur`): iOS and Android 12
 * (API 31) and later. Below that only the scrim shows.
 */
expect fun platformBlursBackdrop(): Boolean

/** How many sheets or dialogs are open over the app's content (MASTER.md "Dialog / sheet"). */
@Stable
class Backdrop {
    var open by mutableIntStateOf(0)
        private set

    internal fun opened() {
        open++
    }

    internal fun closed() {
        open--
    }
}

internal val LocalBackdrop = staticCompositionLocalOf { Backdrop() }

/** Marks a modal as open for as long as it is composed: the content behind it blurs. */
@Composable
fun HoldBackdrop() {
    val backdrop = LocalBackdrop.current
    DisposableEffect(backdrop) {
        backdrop.opened()
        onDispose { backdrop.closed() }
    }
}

/**
 * The app's content, blurred by `--backdrop-blur` while a sheet or dialog is open over it. The
 * radius is static (never animated); where the platform cannot blur, the scrim alone remains.
 */
@Composable
fun Modifier.blurredUnderModals(): Modifier {
    val blurred = LocalBackdrop.current.open > 0 && platformBlursBackdrop()
    return if (blurred) blur(Tokens.backdropBlur) else this
}
