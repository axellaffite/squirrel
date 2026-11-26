package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.unit.dp
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.window.Dialog
import app.squirrel.theme.HoldBackdrop
import app.squirrel.theme.edgeToEdgeDialogProperties
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.boxShadow

/**
 * MASTER.md "Dialog / sheet" on phones: a bottom sheet on `--card` with a 1 dp `--hair` edge,
 * `--radius-panel` top corners, `--shadow-pop`, over the scrim, with a tint close button. While it
 * is open, the app's content behind it is blurred (see [app.squirrel.theme.blurredUnderModals]).
 */
@Composable
fun Sheet(title: String, onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val scheme = SquirrelTheme.scheme
    val shape = RoundedCornerShape(topStart = Tokens.radiusPanel, topEnd = Tokens.radiusPanel)
    HoldBackdrop()
    // Edge to edge: the scrim covers the whole screen and the sheet reaches the bottom edge; the
    // sheet's own navigationBars padding keeps its content clear of the home indicator.
    Dialog(onDismissRequest = onDismiss, properties = edgeToEdgeDialogProperties()) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(scheme.scrim)
                .clickable(remember { MutableInteractionSource() }, indication = null, onClick = onDismiss),
            contentAlignment = Alignment.BottomCenter,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .boxShadow(scheme.shadowPop, shape)
                    .background(scheme.card, shape)
                    .border(1.dp, scheme.hair, shape)
                    .clickable(remember { MutableInteractionSource() }, indication = null) {}
                    .windowInsetsPadding(WindowInsets.navigationBars)
                    .verticalScroll(rememberScrollState())
                    .padding(Tokens.panelPad),
                verticalArrangement = Arrangement.spacedBy(Tokens.space3),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    BasicText(title, style = SquirrelTheme.type.sectionTitle, modifier = Modifier.weight(1f).semantics { heading() })
                    IconButton(SquirrelIcon.X, label = "Close $title", onClick = onDismiss, tinted = true)
                }
                content()
            }
        }
    }
}
