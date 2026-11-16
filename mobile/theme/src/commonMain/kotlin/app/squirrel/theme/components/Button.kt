package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Shapes
import app.squirrel.theme.Tokens
import app.squirrel.theme.boxShadow

enum class ButtonVariant { Primary, Secondary, Quiet, Danger, DangerSolid }

/** MASTER.md button sizes: sm 36 (44 hit area), md 44, lg 48. */
enum class ButtonSize(val height: Dp) {
    Sm(Tokens.controlSm),
    Md(Tokens.controlMd),
    Lg(Tokens.controlLg),
}

/** A pill button (MASTER.md "Button"). The accent glow is on the primary variant only. */
@Composable
fun Button(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    variant: ButtonVariant = ButtonVariant.Primary,
    size: ButtonSize = ButtonSize.Md,
    enabled: Boolean = true,
    full: Boolean = false,
    icon: app.squirrel.theme.SquirrelIcon? = null,
) {
    val scheme = SquirrelTheme.scheme
    val interaction = remember { MutableInteractionSource() }
    val (fill, ink) = when (variant) {
        ButtonVariant.Primary -> scheme.accent to scheme.onAccent
        ButtonVariant.Secondary -> scheme.tint to scheme.ink
        ButtonVariant.Quiet -> Color.Transparent to scheme.inkMuted
        ButtonVariant.Danger -> scheme.tint to scheme.danger
        ButtonVariant.DangerSolid -> scheme.danger to scheme.onDanger
    }
    val style = if (variant == ButtonVariant.Primary) SquirrelTheme.type.strong else SquirrelTheme.type.label
    Box(
        modifier = modifier
            .sizeIn(minWidth = MinTouchTarget, minHeight = MinTouchTarget)
            .then(if (full) Modifier.fillMaxWidth() else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            modifier = Modifier
                .then(if (full) Modifier.fillMaxWidth() else Modifier)
                .controlStates(interaction, Shapes.pill, enabled)
                .then(
                    if (variant == ButtonVariant.Primary && enabled) Modifier.boxShadow(scheme.glow, Shapes.pill)
                    else Modifier,
                )
                .background(fill, Shapes.pill)
                .clickable(interaction, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
                .heightIn(min = size.height)
                .padding(horizontal = Tokens.space4, vertical = Tokens.space2),
            horizontalArrangement = Arrangement.spacedBy(Tokens.space2, Alignment.CenterHorizontally),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (icon != null) Icon(icon, contentDescription = null, tint = ink)
            BasicText(text, style = style.copy(color = ink), maxLines = 1)
        }
    }
}
