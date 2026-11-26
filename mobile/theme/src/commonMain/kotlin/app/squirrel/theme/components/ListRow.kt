package app.squirrel.theme.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.boxShadow
import app.squirrel.theme.tabular

/** MASTER.md "Table": the 32 dp `--tint` icon tile of a row (`--radius-control`). */
val RowTile: Dp = Tokens.space6

/** Where a row sits in its group, for rows laid out one by one (a lazy list) instead of in a [RowGroup]. */
enum class RowPlace {
    Single, First, Middle, Last;

    companion object {
        /** The place of row [index] among [count] rows. */
        fun of(index: Int, count: Int): RowPlace = when {
            count <= 1 -> Single
            index == 0 -> First
            index == count - 1 -> Last
            else -> Middle
        }
    }
}

/**
 * MASTER.md phone "Lists": rows grouped in one `--card` with a 1 dp `--hair` edge, `--radius-card`
 * and the card shadow, with 1 dp `--hair` separators between the rows it lays out.
 */
@Composable
fun RowGroup(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val scheme = SquirrelTheme.scheme
    // Where the separators go: written while laying out, read while drawing.
    var lines by remember { mutableStateOf(IntArray(0)) }
    Layout(
        content = content,
        modifier = modifier
            .fillMaxWidth()
            .boxShadow(scheme.shadowCard, Shapes.card)
            .background(scheme.card, Shapes.card)
            .clip(Shapes.card)
            .drawWithContent {
                drawContent()
                val hair = 1.dp.toPx()
                lines.forEach { y -> drawRect(scheme.hair, Offset(0f, y.toFloat()), Size(size.width, hair)) }
            }
            .border(1.dp, scheme.hair, Shapes.card),
    ) { measurables, constraints ->
        val rows = measurables.map { it.measure(constraints.copy(minHeight = 0, maxHeight = Int.MAX_VALUE)) }
        val width = rows.maxOfOrNull { it.width }?.coerceAtLeast(constraints.minWidth) ?: constraints.minWidth
        val tops = rows.runningFold(0) { y, row -> y + row.height }
        val height = tops.last().coerceIn(constraints.minHeight, constraints.maxHeight)
        val between = tops.drop(1).dropLast(1).toIntArray()
        if (!between.contentEquals(lines)) lines = between
        layout(width, height) { rows.forEachIndexed { i, row -> row.placeRelative(0, tops[i]) } }
    }
}

/**
 * One row of a list (MASTER.md "Table" rows on the phone): a 52 dp row with a 32 dp tinted icon
 * tile, name 500 and muted tabular metadata, actions on the right. It sits inside a [RowGroup];
 * in a lazy list, [place] makes it draw its own part of the group instead. Pressed or hovered,
 * it takes a `--tint` fill; keyboard focus is a 2 dp accent ring inset 4 dp.
 */
@Composable
fun ListRow(
    name: String,
    modifier: Modifier = Modifier,
    icon: SquirrelIcon? = null,
    meta: String? = null,
    onClick: (() -> Unit)? = null,
    /** Replaces the icon tile, e.g. with a [RowTile] thumbnail. */
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
    /** Set in a lazy list (no [RowGroup] around it): the row draws its part of the group. */
    place: RowPlace? = null,
) {
    val scheme = SquirrelTheme.scheme
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    val active = onClick != null && (pressed || hovered)
    val fill by animateColorAsState(if (active) scheme.tint else Color.Transparent, SquirrelTheme.motion.fast())
    Row(
        modifier = modifier
            .fillMaxWidth()
            .then(if (place != null) Modifier.groupPart(place) else Modifier)
            .background(fill)
            .drawWithContent {
                drawContent()
                if (focused) {
                    val ring = Tokens.ringWidth.toPx()
                    val inset = Tokens.space1.toPx() + ring / 2
                    val radius = Tokens.radiusControl.toPx()
                    drawRoundRect(
                        scheme.accent,
                        topLeft = Offset(inset, inset),
                        size = Size(size.width - 2 * inset, size.height - 2 * inset),
                        cornerRadius = CornerRadius(radius),
                        style = Stroke(ring),
                    )
                }
            }
            .then(
                if (onClick != null) Modifier.clickable(interaction, indication = null, role = Role.Button, onClick = onClick)
                else Modifier,
            )
            .heightIn(min = maxOf(Tokens.rowMin, MinTouchTarget))
            .padding(horizontal = Tokens.space3, vertical = Tokens.space2),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space3),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        when {
            leading != null -> leading()
            icon != null -> IconTile(icon)
        }
        Column(Modifier.weight(1f)) {
            BasicText(name, style = SquirrelTheme.type.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (meta != null) BasicText(meta, style = SquirrelTheme.type.caption.tabular(), maxLines = 1)
        }
        trailing?.invoke(this)
    }
}

/** A row's 32 dp `--tint` icon tile with `--radius-control` corners. */
@Composable
fun IconTile(icon: SquirrelIcon, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier.size(RowTile).background(SquirrelTheme.scheme.tint, Shapes.control),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null)
    }
}

/**
 * The part of a [RowGroup] one row draws in a lazy list: the card fill and shadow with the
 * group's corners at its ends, the hair edge (the group's outline, clipped to the row, so the
 * sides meet without doubling), and the separator above every row but the first.
 */
@Composable
private fun Modifier.groupPart(place: RowPlace): Modifier {
    val scheme = SquirrelTheme.scheme
    val radius = Tokens.radiusCard
    val shape: Shape = when (place) {
        RowPlace.Single -> Shapes.card
        RowPlace.First -> RoundedCornerShape(topStart = radius, topEnd = radius)
        RowPlace.Middle -> RoundedCornerShape(0.dp)
        RowPlace.Last -> RoundedCornerShape(bottomStart = radius, bottomEnd = radius)
    }
    val openTop = place == RowPlace.Middle || place == RowPlace.Last
    val openBottom = place == RowPlace.First || place == RowPlace.Middle
    return this
        .boxShadow(scheme.shadowCard, shape)
        .background(scheme.card, shape)
        .clip(shape)
        .drawWithContent {
            drawContent()
            val hair = 1.dp.toPx()
            val r = radius.toPx()
            // The group's outline runs past an open end, so only its sides show there.
            val top = if (openTop) -2 * r else 0f
            val bottom = if (openBottom) size.height + 2 * r else size.height
            clipRect {
                drawRoundRect(
                    scheme.hair,
                    topLeft = Offset(hair / 2, top + hair / 2),
                    size = Size(size.width - hair, bottom - top - hair),
                    cornerRadius = CornerRadius(r - hair / 2),
                    style = Stroke(hair),
                )
            }
            if (openTop) drawRect(scheme.hair, Offset.Zero, Size(size.width, hair))
        }
}
