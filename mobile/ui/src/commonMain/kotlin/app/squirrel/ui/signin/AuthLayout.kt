package app.squirrel.ui.signin

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.cardShadow
import androidx.compose.foundation.background

/** MASTER.md auth pages: the wordmark and one white card on the ground. */
@Composable
fun AuthLayout(title: String, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(Tokens.gutter),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Tokens.space5, Alignment.CenterVertically),
    ) {
        BasicText("Squirrel", style = SquirrelTheme.type.wordmark)
        Column(
            modifier = Modifier
                .widthIn(max = Tokens.authMax)
                .fillMaxWidth()
                .cardShadow(Shapes.panel)
                .background(SquirrelTheme.scheme.card, Shapes.panel)
                .padding(Tokens.panelPad),
            verticalArrangement = Arrangement.spacedBy(Tokens.space4),
        ) {
            BasicText(title, style = SquirrelTheme.type.sectionTitle, modifier = Modifier.semantics { heading() })
            content()
        }
    }
}
