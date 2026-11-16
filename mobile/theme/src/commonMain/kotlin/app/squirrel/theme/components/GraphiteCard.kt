package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.cardShadow

/**
 * MASTER.md: at most one graphite card per screen, for a featured figure (`--rail` gradient,
 * `--rail-ink` text, `--radius-card`). The Backup screen's status card.
 */
@Composable
fun GraphiteCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .cardShadow(Shapes.card)
            .background(SquirrelTheme.scheme.rail.brush(), Shapes.card)
            .padding(Tokens.panelPad),
        verticalArrangement = Arrangement.spacedBy(Tokens.space2),
        content = content,
    )
}
