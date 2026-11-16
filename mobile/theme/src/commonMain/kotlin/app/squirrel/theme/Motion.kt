package app.squirrel.theme

import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable

/**
 * MASTER.md "Motion": three durations on one ease. With reduced motion every duration is 0.
 * List and section arrival is skipped on phones (touch devices), so there is no stagger here.
 */
@Immutable
class Motion(val reduced: Boolean) {
    val fastMillis: Int get() = if (reduced) 0 else Tokens.durFast
    val baseMillis: Int get() = if (reduced) 0 else Tokens.dur
    val slowMillis: Int get() = if (reduced) 0 else Tokens.durSlow

    /** Presses, colour and background changes, toggles. */
    fun <T> fast(): FiniteAnimationSpec<T> = tween(fastMillis, easing = Tokens.ease)

    /** Card and row shadows, menus. */
    fun <T> base(): FiniteAnimationSpec<T> = tween(baseMillis, easing = Tokens.ease)

    /** Sheets and dialogs. */
    fun <T> slow(): FiniteAnimationSpec<T> = tween(slowMillis, easing = Tokens.ease)
}

/** The system's reduced-motion setting (Android animator scale 0, iOS Reduce Motion). */
@Composable
expect fun systemReducesMotion(): Boolean
