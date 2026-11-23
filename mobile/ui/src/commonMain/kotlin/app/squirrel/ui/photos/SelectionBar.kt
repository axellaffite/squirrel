package app.squirrel.ui.photos

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.popShadow
import app.squirrel.theme.tabular

/**
 * MASTER "Selection bar": a `--card` pill with `--shadow-pop`, floating above the phone rail: the
 * count (tabular), then small actions, then a way out of selecting.
 */
@Composable
fun SelectionBar(count: Int, onClear: () -> Unit, modifier: Modifier = Modifier, actions: @Composable RowScope.() -> Unit) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(bottom = RailClearance)
            .popShadow(Shapes.pill)
            .background(SquirrelTheme.scheme.card, Shapes.pill)
            .padding(horizontal = Tokens.space2, vertical = Tokens.space1),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space1),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(SquirrelIcon.X, label = "Clear the selection", onClick = onClear)
        BasicText(
            "$count selected",
            style = SquirrelTheme.type.label.tabular(),
            modifier = Modifier.weight(1f).semantics { liveRegion = LiveRegionMode.Polite },
        )
        actions()
    }
}

/** A selection of photos by id: a long press starts it, a tap toggles while it lasts. */
fun Set<String>.toggled(id: String): Set<String> = if (id in this) this - id else this + id
