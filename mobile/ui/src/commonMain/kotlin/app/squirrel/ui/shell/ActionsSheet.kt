package app.squirrel.ui.shell

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Icon
import app.squirrel.theme.components.MinTouchTarget
import app.squirrel.theme.components.Sheet

/** One choice of an actions sheet; [label] names it with its subject for assistive tech. */
data class SheetAction(val text: String, val icon: SquirrelIcon, val label: String, val danger: Boolean = false, val run: () -> Unit)

/**
 * The phone's actions sheet of a row or a page ("Actions for report.pdf"): each action closes
 * the sheet, then runs, so the next sheet starts clean.
 */
@Composable
fun ActionsSheet(title: String, actions: List<SheetAction>, onClose: () -> Unit) {
    Sheet(title, onDismiss = onClose) {
        actions.forEach { action ->
            ActionRow(action) {
                onClose()
                action.run()
            }
        }
    }
}

@Composable
private fun ActionRow(action: SheetAction, onClick: () -> Unit) {
    val scheme = SquirrelTheme.scheme
    val ink = if (action.danger) scheme.danger else scheme.ink
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = MinTouchTarget)
            .clip(Shapes.card)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = action.label }
            .padding(vertical = Tokens.space1),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space3),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(Tokens.wellSm).background(scheme.tint, Shapes.circle), contentAlignment = Alignment.Center) {
            Icon(action.icon, contentDescription = null, tint = ink)
        }
        BasicText(action.text, style = SquirrelTheme.type.label.copy(color = ink))
    }
}
