package app.squirrel.theme

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp

/** A two-stop vertical CSS gradient (`linear-gradient(180deg, top 0%, bottom 100%)`). */
@Immutable
data class GradientToken(val top: Color, val bottom: Color) {
    fun brush(): Brush = Brush.verticalGradient(listOf(top, bottom))
}

/** One layer of a CSS `box-shadow`. */
@Immutable
data class ShadowLayer(
    val x: Dp,
    val y: Dp,
    val blur: Dp,
    val spread: Dp,
    val color: Color,
    val inset: Boolean,
)
