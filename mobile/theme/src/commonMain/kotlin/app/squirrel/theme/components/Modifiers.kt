package app.squirrel.theme.components

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.shape.CornerBasedShape
import androidx.compose.foundation.shape.CornerSize
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens

/** MASTER.md phone app: every interactive element is at least 48 dp (`--control-lg`). */
val MinTouchTarget: Dp = Tokens.controlLg

/** The page ground: `--ground` gradient. */
@Composable
fun Modifier.ground(): Modifier = background(SquirrelTheme.scheme.ground.brush())

/**
 * MASTER.md focus: a 2 px ring, 2 px outside [shape], when [focused] (keyboard focus). The ring is
 * concentric with the element: each corner grows by the ring's distance (outer radius = radius + gap),
 * so an 8 dp field gets a 10 dp ring and a pill stays a pill.
 */
fun Modifier.focusRing(focused: Boolean, shape: Shape, color: Color): Modifier = drawWithContent {
    drawContent()
    if (focused) {
        val width = Tokens.ringWidth.toPx()
        val inset = Tokens.ringOffset.toPx() + width / 2
        val ringShape = shape.grownBy(inset, size, this)
        val outline = ringShape.createOutline(Size(size.width + 2 * inset, size.height + 2 * inset), layoutDirection, this)
        translate(-inset, -inset) { drawOutline(outline, color, style = Stroke(width)) }
    }
}

/** [this] with every corner radius grown by [by] px; shapes without corners are returned as they are. */
private fun Shape.grownBy(by: Float, size: Size, density: Density): Shape {
    if (this !is CornerBasedShape) return this
    fun CornerSize.grown() = CornerSize(toPx(size, density) + by)
    return RoundedCornerShape(topStart.grown(), topEnd.grown(), bottomEnd.grown(), bottomStart.grown())
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
