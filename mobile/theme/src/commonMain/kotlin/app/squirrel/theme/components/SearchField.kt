package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens

/**
 * MASTER.md "Search field": the text field pill with a leading 20 dp `search` icon in
 * `--ink-muted` and a visually hidden [label] (read by assistive tech). A clear button appears
 * once something is typed; the keyboard's search key calls [onSearch].
 */
@Composable
fun SearchField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    placeholder: String = label,
    onSearch: () -> Unit = {},
    autofocus: Boolean = false,
) {
    val scheme = SquirrelTheme.scheme
    val type = SquirrelTheme.type
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val focus = remember { FocusRequester() }
    if (autofocus) LaunchedEffect(Unit) { focus.requestFocus() }
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        singleLine = true,
        textStyle = type.body,
        cursorBrush = SolidColor(scheme.accent),
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        keyboardActions = KeyboardActions(onSearch = { onSearch() }),
        interactionSource = interaction,
        modifier = modifier.fillMaxWidth().focusRequester(focus).semantics { contentDescription = label },
        decorationBox = { inner ->
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = MinTouchTarget)
                    .focusRing(focused, Shapes.pill, scheme.accent)
                    .background(scheme.card, Shapes.pill)
                    .border(1.dp, scheme.edge, Shapes.pill)
                    .padding(start = Tokens.space4, end = Tokens.space1),
                horizontalArrangement = Arrangement.spacedBy(Tokens.space2),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(SquirrelIcon.Search, contentDescription = null, tint = scheme.inkMuted)
                Box(Modifier.weight(1f).padding(vertical = Tokens.space2)) {
                    if (value.isEmpty()) BasicText(placeholder, style = type.body.copy(color = scheme.inkMuted), maxLines = 1)
                    inner()
                }
                if (value.isNotEmpty()) IconButton(SquirrelIcon.X, label = "Clear the search", onClick = { onValueChange("") })
            }
        },
    )
}
