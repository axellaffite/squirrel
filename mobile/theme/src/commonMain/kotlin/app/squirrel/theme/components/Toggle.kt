package app.squirrel.theme.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.boxShadow

// The web's Toggle.svelte: track space-6 + space-2 wide, space-5 high; knob space-4 + space-1 / 2.
private val TrackWidth = Tokens.space6 + Tokens.space2
private val TrackHeight = Tokens.space5
private val Knob = Tokens.space4 + Tokens.space1 / 2

/** A labelled switch: `--tint` track with a 1 px `--edge`, white knob; on fills with `--accent`. */
@Composable
fun Toggle(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    description: String? = null,
    enabled: Boolean = true,
) {
    val scheme = SquirrelTheme.scheme
    val motion = SquirrelTheme.motion
    val interaction = remember { MutableInteractionSource() }
    val track by animateColorAsState(if (checked) scheme.accent else scheme.tint, motion.fast())
    val edge by animateColorAsState(if (checked) scheme.accent else scheme.edge, motion.fast())
    val knob by animateColorAsState(if (checked) scheme.onAccent else scheme.card, motion.fast())
    val travel = TrackWidth - Knob - Tokens.space1 * 2
    val x by animateDpAsState(if (checked) travel else 0.dp, motion.fast())
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget)
            .toggleable(checked, interaction, indication = null, enabled = enabled, role = Role.Switch, onValueChange = onCheckedChange)
            .controlStates(interaction, Shapes.small, enabled),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space3),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            BasicText(label, style = SquirrelTheme.type.body)
            if (description != null) BasicText(description, style = SquirrelTheme.type.caption)
        }
        Box(
            modifier = Modifier
                .width(TrackWidth)
                .height(TrackHeight)
                .background(track, Shapes.pill)
                .border(1.dp, edge, Shapes.pill),
            contentAlignment = Alignment.CenterStart,
        ) {
            Box(
                Modifier
                    .offset(x = Tokens.space1 + x)
                    .size(Knob)
                    .boxShadow(scheme.shadowKnob, Shapes.circle)
                    .background(knob, Shapes.circle),
            )
        }
    }
}
