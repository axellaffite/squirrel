package app.squirrel.theme.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.boxShadow

/** One destination of the bar. */
data class RailItem(val key: String, val label: String, val icon: SquirrelIcon)

/** One choice of a menu (the bar's add menu). */
data class MenuAction(val text: String, val icon: SquirrelIcon, val run: () -> Unit)

/**
 * MASTER.md "Bottom bar": the notch is 20 × 3 dp with 3 dp bottom corners, 32 dp wide for keyboard
 * focus. No token names these sizes, so they are built from the scale they sit on.
 */
private val NotchW = Tokens.space5 - Tokens.space1
private val NotchFocusW = Tokens.space6
private val NotchH = Tokens.press * 3
private val NotchRadius = NotchH

/** Tab icons are 22 dp: between `--icon-sm` and `--icon-md`. */
private val TabIcon = (Tokens.iconSm + Tokens.iconMd) / 2

/** The add button fills the bar's inner height, less 2 dp. */
private val AddDiameter = Tokens.railBarH - Tokens.railBarPad * 2 - Tokens.press * 2

/** The add icon turns a quarter of a right angle while its menu is open. */
private const val ADD_TURN = 45f

/** The menu's arrow, pointing down at the add button. */
private val ArrowW = Tokens.space4
private val ArrowH = Tokens.space2

/**
 * MASTER.md phone "Bottom bar": a floating white full pill (`--rail-bar-h`, `--rail-bar-pad`,
 * `--card`, 1 dp `--hair`, `--shadow-hover`) with five equal columns: the first half of [items],
 * the Orchid add button (its menu of [addActions] opens above the bar), then the rest. Tabs are
 * icon-only; their every state is a top notch hanging just inside the top hairline.
 */
@Composable
fun BottomRail(
    items: List<RailItem>,
    selected: String,
    onSelect: (String) -> Unit,
    addLabel: String,
    addActions: List<MenuAction>,
    modifier: Modifier = Modifier,
) {
    val scheme = SquirrelTheme.scheme
    val half = (items.size + 1) / 2
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(Tokens.railBarH)
            .boxShadow(scheme.shadowHover, Shapes.pill)
            .background(scheme.card, Shapes.pill)
            .border(1.dp, scheme.hair, Shapes.pill)
            .padding(horizontal = Tokens.railBarPad)
            .selectableGroup(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        items.take(half).forEach { item -> RailTab(item, item.key == selected) { onSelect(item.key) } }
        AddButton(addLabel, addActions)
        items.drop(half).forEach { item -> RailTab(item, item.key == selected) { onSelect(item.key) } }
    }
}

@Composable
private fun RowScope.RailTab(item: RailItem, selected: Boolean, onClick: () -> Unit) {
    val scheme = SquirrelTheme.scheme
    val motion = SquirrelTheme.motion
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    // Focus, then press, selection and hover: one notch shows the strongest state.
    val notch = when {
        focused -> scheme.ink
        pressed || selected -> scheme.accent
        hovered -> scheme.edge
        else -> null
    }
    val ink = when {
        pressed || selected -> scheme.accentInk
        hovered || focused -> scheme.ink
        else -> scheme.inkMuted
    }
    val notchColor by animateColorAsState(notch ?: scheme.accent, motion.fast())
    val notchAlpha by animateFloatAsState(if (notch != null) 1f else 0f, motion.fast())
    val iconColor by animateColorAsState(ink, motion.fast())
    val sink by animateDpAsState(if (pressed) Tokens.press else 0.dp, motion.fast())
    // Width is never animated (MASTER.md "Motion"): focus switches the notch to 32 dp at once.
    val notchWidth = if (focused) NotchFocusW else NotchW
    Box(
        modifier = Modifier
            .weight(1f)
            .fillMaxHeight()
            .heightIn(min = MinTouchTarget)
            .hoverable(interaction)
            .selectable(selected, interaction, indication = null, role = Role.Tab, onClick = onClick)
            .semantics { contentDescription = item.label }
            .drawBehind {
                if (notchAlpha > 0f) {
                    val w = notchWidth.toPx()
                    val h = NotchH.toPx()
                    val r = NotchRadius.toPx()
                    // Just inside the bar's top hairline (1 dp), rounded at the bottom only.
                    val path = Path().apply {
                        addRoundRect(
                            RoundRect(
                                left = (size.width - w) / 2,
                                top = 1.dp.toPx(),
                                right = (size.width + w) / 2,
                                bottom = 1.dp.toPx() + h,
                                bottomLeftCornerRadius = CornerRadius(r),
                                bottomRightCornerRadius = CornerRadius(r),
                            ),
                        )
                    }
                    drawPath(path, notchColor.copy(alpha = notchColor.alpha * notchAlpha))
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        Icon(item.icon, contentDescription = null, tint = iconColor, size = TabIcon, modifier = Modifier.offset(y = sink))
    }
}

@Composable
private fun RowScope.AddButton(label: String, actions: List<MenuAction>) {
    val scheme = SquirrelTheme.scheme
    var open by remember { mutableStateOf(false) }
    val turn by animateFloatAsState(if (open) ADD_TURN else 0f, SquirrelTheme.motion.base())
    val interaction = remember { MutableInteractionSource() }
    Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.Center) {
        Box(
            modifier = Modifier
                .size(AddDiameter)
                .controlStates(interaction, Shapes.circle, enabled = true)
                .boxShadow(scheme.shadowHover, Shapes.circle)
                .background(scheme.accent, Shapes.circle)
                .clickable(interaction, indication = null, role = Role.Button) { open = !open }
                .semantics { contentDescription = label },
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                SquirrelIcon.Plus,
                contentDescription = null,
                tint = scheme.onAccent,
                size = Tokens.iconMd,
                modifier = Modifier.graphicsLayer { rotationZ = turn },
            )
            if (open) {
                val gap = with(LocalDensity.current) { (Tokens.railBarPad + Tokens.space2).roundToPx() }
                Popup(
                    popupPositionProvider = remember(gap) { AboveAnchor(gap) },
                    onDismissRequest = { open = false },
                    properties = PopupProperties(focusable = true),
                ) {
                    AddMenu(actions) { action ->
                        open = false
                        action.run()
                    }
                }
            }
        }
    }
}

/** Centres the popup over its anchor, [gap] px above it, kept inside the window. */
private class AboveAnchor(private val gap: Int) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val x = (anchorBounds.center.x - popupContentSize.width / 2).coerceIn(0, maxOf(0, windowSize.width - popupContentSize.width))
        val y = (anchorBounds.top - gap - popupContentSize.height).coerceAtLeast(0)
        return IntOffset(x, y)
    }
}

/**
 * MASTER.md "Menu" above the bar: `--card`, 1 dp `--hair`, `--radius-card`, 4 dp padding,
 * `--radius-sm` items, `--shadow-pop`, and an arrow pointing down at the add button. It arrives
 * like a menu (`--dur`, rising `--rise`), at once with reduced motion.
 */
@Composable
private fun AddMenu(actions: List<MenuAction>, onPick: (MenuAction) -> Unit) {
    val scheme = SquirrelTheme.scheme
    val motion = SquirrelTheme.motion
    val arrival = remember { Animatable(0f) }
    LaunchedEffect(Unit) { arrival.animateTo(1f, motion.base()) }
    val rise = with(LocalDensity.current) { Tokens.rise.toPx() }
    Column(
        modifier = Modifier.graphicsLayer {
            alpha = arrival.value
            translationY = rise * (1f - arrival.value)
        },
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(
            modifier = Modifier
                .width(IntrinsicSize.Max)
                .boxShadow(scheme.shadowPop, Shapes.card)
                .background(scheme.card, Shapes.card)
                .border(1.dp, scheme.hair, Shapes.card)
                .padding(Tokens.space1),
        ) {
            actions.forEach { action -> MenuRow(action) { onPick(action) } }
        }
        MenuArrow()
    }
}

@Composable
private fun MenuRow(action: MenuAction, onClick: () -> Unit) {
    val scheme = SquirrelTheme.scheme
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val hovered by interaction.collectIsHoveredAsState()
    val focused by interaction.collectIsFocusedAsState()
    val fill by animateColorAsState(if (pressed || hovered) scheme.tint else Color.Transparent, SquirrelTheme.motion.fast())
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget)
            .focusRing(focused, Shapes.small, scheme.accent)
            .clip(Shapes.small)
            .background(fill)
            .hoverable(interaction)
            .clickable(interaction, indication = null, role = Role.Button, onClick = onClick)
            .padding(horizontal = Tokens.space3),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space3),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(action.icon, contentDescription = null, tint = scheme.inkMuted)
        BasicText(action.text, style = SquirrelTheme.type.label, maxLines = 1, modifier = Modifier.padding(end = Tokens.space2))
    }
}

/** A small card-coloured triangle under the menu, its two lower sides drawn in `--hair`. */
@Composable
private fun MenuArrow() {
    val scheme = SquirrelTheme.scheme
    // Overlaps the menu's bottom edge by 1 dp, so the hairline breaks where the arrow leaves it.
    Canvas(Modifier.size(ArrowW, ArrowH).offset(y = -Tokens.press).clearAndSetSemantics {}) {
        val path = Path().apply {
            moveTo(0f, 0f)
            lineTo(size.width / 2, size.height)
            lineTo(size.width, 0f)
            close()
        }
        drawPath(path, scheme.card)
        val edge = Path().apply {
            moveTo(0f, 0f)
            lineTo(size.width / 2, size.height)
            lineTo(size.width, 0f)
        }
        drawPath(edge, scheme.hair, style = Stroke(1.dp.toPx()))
    }
}
