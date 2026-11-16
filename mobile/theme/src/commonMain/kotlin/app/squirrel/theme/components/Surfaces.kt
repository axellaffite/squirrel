package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.cardShadow

/** A white card with the diffuse card shadow. */
@Composable
fun Card(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = modifier
            .cardShadow(Shapes.card)
            .background(SquirrelTheme.scheme.card, Shapes.card)
            .padding(Tokens.cardPad),
        content = content,
    )
}

/** A tinted well: holds row cards, forms, the storage meter. */
@Composable
fun Well(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = modifier
            .background(SquirrelTheme.scheme.tint, Shapes.well)
            .padding(Tokens.wellPad),
        content = content,
    )
}

/** A round icon well (tinted, or white with the card shadow). */
@Composable
fun IconWell(
    icon: SquirrelIcon,
    modifier: Modifier = Modifier,
    white: Boolean = false,
    size: androidx.compose.ui.unit.Dp = Tokens.wellSm,
) {
    val scheme = SquirrelTheme.scheme
    Box(
        modifier = modifier
            .size(size)
            .then(if (white) Modifier.cardShadow(Shapes.circle) else Modifier)
            .background(if (white) scheme.card else scheme.tint, Shapes.circle),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null)
    }
}

/** An empty state: a tinted well with a white icon well, a title and a muted line. */
@Composable
fun EmptyState(icon: SquirrelIcon, title: String, line: String, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(SquirrelTheme.scheme.tint, Shapes.well)
            .padding(Tokens.space6),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        IconWell(icon, white = true, size = Tokens.wellLg)
        Spacer(Modifier.height(Tokens.space4))
        BasicText(title, style = SquirrelTheme.type.cardTitle.copy(textAlign = TextAlign.Center))
        Spacer(Modifier.height(Tokens.space1))
        BasicText(line, style = SquirrelTheme.type.caption.copy(textAlign = TextAlign.Center))
    }
}
