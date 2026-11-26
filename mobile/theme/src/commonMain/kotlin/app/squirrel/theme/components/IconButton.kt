package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens

/** A 36 dp `--radius-control` button with a 48 dp hit area; [label] names its subject ("Account menu"). */
@Composable
fun IconButton(
    icon: SquirrelIcon,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    tinted: Boolean = false,
    enabled: Boolean = true,
) {
    require(label.isNotBlank()) { "An icon-only button needs an accessible name" }
    val scheme = SquirrelTheme.scheme
    val interaction = remember { MutableInteractionSource() }
    Box(
        modifier = modifier
            .sizeIn(minWidth = MinTouchTarget, minHeight = MinTouchTarget)
            .clickable(interaction, indication = null, enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Box(
            modifier = Modifier
                .size(Tokens.controlSm)
                .controlStates(interaction, Shapes.control, enabled)
                .background(if (tinted) scheme.tint else Color.Transparent, Shapes.control),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = null, tint = scheme.inkMuted)
        }
    }
}
