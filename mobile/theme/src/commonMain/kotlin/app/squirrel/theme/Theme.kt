package app.squirrel.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf

/** Everything a component reads from the theme. */
@Immutable
class ThemeValues(
    val scheme: SchemeTokens,
    val dark: Boolean,
    val motion: Motion,
    val type: SquirrelType,
)

/** The light or dark token set (MASTER.md "Theme": follows the system). */
fun schemeFor(dark: Boolean): SchemeTokens = if (dark) Tokens.Dark else Tokens.Light

private val LocalTheme = staticCompositionLocalOf<ThemeValues> {
    error("SquirrelTheme { } is missing above this composable")
}

/** Squirrel's design system for Compose, derived from MASTER.md. */
@Composable
fun SquirrelTheme(
    dark: Boolean = isSystemInDarkTheme(),
    reduceMotion: Boolean = systemReducesMotion(),
    content: @Composable () -> Unit,
) {
    val scheme = schemeFor(dark)
    val values = ThemeValues(
        scheme = scheme,
        dark = dark,
        motion = Motion(reduceMotion),
        type = squirrelType(geistFamily(), scheme),
    )
    CompositionLocalProvider(
        LocalTheme provides values,
        LocalBackdrop provides remember { Backdrop() },
        LocalTextSelectionColors provides TextSelectionColors(
            handleColor = scheme.accent,
            backgroundColor = scheme.accentSoft,
        ),
        content = content,
    )
}

/** Access to the current theme: `SquirrelTheme.scheme.accent`, `SquirrelTheme.type.body`. */
object SquirrelTheme {
    val scheme: SchemeTokens
        @Composable get() = LocalTheme.current.scheme
    val type: SquirrelType
        @Composable get() = LocalTheme.current.type
    val motion: Motion
        @Composable get() = LocalTheme.current.motion
    val isDark: Boolean
        @Composable get() = LocalTheme.current.dark
}
