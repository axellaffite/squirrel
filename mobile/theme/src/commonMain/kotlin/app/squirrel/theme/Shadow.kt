package app.squirrel.theme

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.draw.innerShadow
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.unit.DpOffset

/** Draws a CSS `box-shadow` (several layers, outer or inset) behind or inside [shape]. */
fun Modifier.boxShadow(layers: List<ShadowLayer>, shape: Shape): Modifier =
    layers.fold(this) { modifier, layer ->
        val shadow = Shadow(
            radius = layer.blur,
            color = layer.color,
            spread = layer.spread,
            offset = DpOffset(layer.x, layer.y),
        )
        if (layer.inset) modifier.innerShadow(shape, shadow) else modifier.dropShadow(shape, shadow)
    }

/** `--shadow-card`: cards, row cards, white icon wells. */
@Composable
fun Modifier.cardShadow(shape: Shape = Shapes.card): Modifier = boxShadow(SquirrelTheme.scheme.shadowCard, shape)

/** `--shadow-pop`: dialogs, menus, the floating phone rail. */
@Composable
fun Modifier.popShadow(shape: Shape = Shapes.rail): Modifier = boxShadow(SquirrelTheme.scheme.shadowPop, shape)
