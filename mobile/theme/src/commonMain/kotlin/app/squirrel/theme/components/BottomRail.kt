package app.squirrel.theme.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.boxShadow
import app.squirrel.theme.popShadow

/** One destination of the rail. */
data class RailItem(val key: String, val label: String, val icon: SquirrelIcon)

/**
 * MASTER.md phone shell: the floating graphite bottom rail (`--rail`, `--radius-rail`,
 * `--shadow-pop`). Active item: white circle, orchid icon, `--shadow-active`, label 600.
 */
@Composable
fun BottomRail(items: List<RailItem>, selected: String, onSelect: (String) -> Unit, modifier: Modifier = Modifier) {
    val scheme = SquirrelTheme.scheme
    Row(
        modifier = modifier
            .fillMaxWidth()
            .popShadow(Shapes.rail)
            .background(scheme.rail.brush(), Shapes.rail)
            .height(Tokens.railBarH)
            .padding(horizontal = Tokens.space2)
            .selectableGroup(),
        horizontalArrangement = Arrangement.SpaceEvenly,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        items.forEach { item -> RailButton(item, item.key == selected) { onSelect(item.key) } }
    }
}

@Composable
private fun RailButton(item: RailItem, active: Boolean, onClick: () -> Unit) {
    val scheme = SquirrelTheme.scheme
    val interaction = remember { MutableInteractionSource() }
    val well by animateColorAsState(if (active) scheme.railActive else scheme.railWell, SquirrelTheme.motion.fast())
    Column(
        modifier = Modifier
            .selectable(active, interaction, indication = null, role = Role.Tab, onClick = onClick)
            .padding(horizontal = Tokens.space3, vertical = Tokens.space1),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Tokens.space1),
    ) {
        Box(
            modifier = Modifier
                .size(Tokens.controlSm)
                .controlStates(interaction, Shapes.circle, enabled = true)
                .then(if (active) Modifier.boxShadow(scheme.shadowActive, Shapes.circle) else Modifier)
                .background(well, Shapes.circle),
            contentAlignment = Alignment.Center,
        ) {
            Icon(item.icon, contentDescription = null, tint = if (active) scheme.railActiveInk else scheme.railMuted)
        }
        val style = SquirrelTheme.type.badge
        BasicText(
            item.label,
            style = if (active) style.copy(color = scheme.railInk, fontWeight = Tokens.weightStrong)
            else style.copy(color = scheme.railMuted),
        )
    }
}
