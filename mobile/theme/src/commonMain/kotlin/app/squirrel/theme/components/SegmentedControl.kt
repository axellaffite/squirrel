package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.boxShadow

/**
 * MASTER.md "Segmented control": a `--tint` pill track; the chosen segment is a `--card` pill
 * with `--shadow-knob` and an `--ink` label, the others `--ink-muted`.
 */
@Composable
fun SegmentedControl(options: List<String>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
    val scheme = SquirrelTheme.scheme
    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(scheme.tint, Shapes.pill)
            .padding(Tokens.space1)
            .selectableGroup(),
    ) {
        options.forEachIndexed { index, label ->
            val chosen = index == selected
            val interaction = remember { MutableInteractionSource() }
            Box(
                modifier = Modifier
                    .weight(1f)
                    .heightIn(min = MinTouchTarget - Tokens.space1 * 2)
                    .controlStates(interaction, Shapes.pill, enabled = true)
                    .then(if (chosen) Modifier.boxShadow(scheme.shadowKnob, Shapes.pill).background(scheme.card, Shapes.pill) else Modifier)
                    .selectable(chosen, interaction, indication = null, role = Role.Tab) { onSelect(index) },
                contentAlignment = Alignment.Center,
            ) {
                BasicText(label, style = SquirrelTheme.type.label.copy(color = if (chosen) scheme.ink else scheme.inkMuted))
            }
        }
    }
}
