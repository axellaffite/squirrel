package app.squirrel.theme.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.unit.Dp
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens

/** MASTER.md "Icons": 1.75 px stroke on the 24-unit grid (the web's Icon.svelte). */
private const val ICON_STROKE = 1.75f
private const val ICON_GRID = 24f

private val vectors = mutableMapOf<SquirrelIcon, ImageVector>()

/** The icon as a stroked vector (tinted when drawn). */
fun SquirrelIcon.imageVector(): ImageVector = vectors.getOrPut(this) {
    ImageVector.Builder(
        name = name,
        defaultWidth = Tokens.iconMd,
        defaultHeight = Tokens.iconMd,
        viewportWidth = ICON_GRID,
        viewportHeight = ICON_GRID,
    ).apply {
        paths.forEach { d ->
            addPath(
                pathData = addPathNodes(d),
                stroke = SolidColor(Color.Black),
                strokeLineWidth = ICON_STROKE,
                strokeLineCap = StrokeCap.Round,
                strokeLineJoin = StrokeJoin.Round,
            )
        }
    }.build()
}

/** An icon, decorative unless [contentDescription] is given. */
@Composable
fun Icon(
    icon: SquirrelIcon,
    contentDescription: String?,
    modifier: Modifier = Modifier,
    tint: Color = SquirrelTheme.scheme.ink,
    size: Dp = Tokens.iconSm,
) {
    Image(
        painter = rememberVectorPainter(icon.imageVector()),
        contentDescription = contentDescription,
        modifier = modifier.size(size),
        colorFilter = ColorFilter.tint(tint),
    )
}
