package app.squirrel.theme.components

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens

/** MASTER.md phone app: every interactive element is at least 48 dp (`--control-lg`). */
val MinTouchTarget: Dp = Tokens.controlLg

/** The page ground: `--ground` gradient. */
@Composable
fun Modifier.ground(): Modifier = background(SquirrelTheme.scheme.ground.brush())

/** MASTER.md focus: a 2 px ring, 2 px outside [shape], when [focused] (keyboard focus). */
fun Modifier.focusRing(focused: Boolean, shape: Shape, color: Color): Modifier = drawWithContent {
    drawContent()
    if (focused) {
        val width = Tokens.ringWidth.toPx()
        val inset = Tokens.ringOffset.toPx() + width / 2
        val outline = shape.createOutline(Size(size.width + 2 * inset, size.height + 2 * inset), layoutDirection, this)
        translate(-inset, -inset) { drawOutline(outline, color, style = Stroke(width)) }
    }
}

/** Focus ring and the 1 px press of every control, read from [interaction]. */
@Composable
fun Modifier.controlStates(interaction: MutableInteractionSource, shape: Shape, enabled: Boolean): Modifier {
    val focused by interaction.collectIsFocusedAsState()
    val pressed by interaction.collectIsPressedAsState()
    val press by animateDpAsState(if (pressed && enabled) Tokens.press else 0.dp, SquirrelTheme.motion.fast())
    return this
        .offset(y = press)
        .focusRing(focused, shape, SquirrelTheme.scheme.accent)
        .alpha(if (enabled) 1f else Tokens.disabledOpacity)
}
