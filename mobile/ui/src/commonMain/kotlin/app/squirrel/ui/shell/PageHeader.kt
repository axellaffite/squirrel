package app.squirrel.ui.shell

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.IconButton

/**
 * MASTER.md phone section: one header per page, the page title (`--text-2xl`); a sub-page puts a
 * round back icon button before its own title instead of repeating the parent's.
 */
@Composable
fun PageHeader(
    title: String,
    onBack: (() -> Unit)? = null,
    backLabel: String = "Back",
    /** Round actions after the title (search, share, comments…). */
    actions: @Composable RowScope.() -> Unit = {},
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = Tokens.space3),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space1),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (onBack != null) IconButton(SquirrelIcon.ChevronLeft, label = backLabel, onClick = onBack)
        BasicText(
            title,
            style = SquirrelTheme.type.pageTitle,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f).semantics { heading() },
        )
        actions()
    }
}
