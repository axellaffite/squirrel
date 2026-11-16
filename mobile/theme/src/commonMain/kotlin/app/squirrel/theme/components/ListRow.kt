package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.cardShadow
import app.squirrel.theme.tabular

/** MASTER.md "Row card": white, 54 dp min, a round tinted icon well, name and muted metadata. */
@Composable
fun ListRow(
    name: String,
    modifier: Modifier = Modifier,
    icon: SquirrelIcon? = null,
    meta: String? = null,
    onClick: (() -> Unit)? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    val interaction = remember { MutableInteractionSource() }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .controlStates(interaction, Shapes.card, enabled = true)
            .cardShadow(Shapes.card)
            .background(SquirrelTheme.scheme.card, Shapes.card)
            .then(
                if (onClick != null) Modifier.clickable(interaction, indication = null, role = Role.Button, onClick = onClick)
                else Modifier,
            )
            .heightIn(min = maxOf(Tokens.rowMin, MinTouchTarget))
            .padding(horizontal = Tokens.space3, vertical = Tokens.space2),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space3),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) IconWell(icon)
        Column(Modifier.weight(1f)) {
            BasicText(name, style = SquirrelTheme.type.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (meta != null) BasicText(meta, style = SquirrelTheme.type.caption.tabular(), maxLines = 1)
        }
        trailing?.invoke(this)
    }
}
