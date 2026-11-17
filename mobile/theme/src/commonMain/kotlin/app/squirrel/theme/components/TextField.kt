package app.squirrel.theme.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.error
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens

/**
 * MASTER.md "Text field": a pill on `--card` with a 1 px `--edge`; invalid shows a `--danger`
 * edge plus an icon and the [error] message (never colour alone).
 */
@Composable
fun TextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    error: String? = null,
    password: Boolean = false,
    enabled: Boolean = true,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    /** What the field holds, for password managers and the system's credential autofill. */
    autofill: ContentType? = null,
) {
    val scheme = SquirrelTheme.scheme
    val type = SquirrelTheme.type
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val edge = if (error != null) scheme.danger else scheme.edge
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Tokens.space1)) {
        BasicText(label, style = type.fieldLabel, modifier = Modifier.padding(start = Tokens.space2))
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            enabled = enabled,
            singleLine = true,
            textStyle = type.body,
            cursorBrush = SolidColor(scheme.accent),
            visualTransformation = if (password) PasswordVisualTransformation() else VisualTransformation.None,
            keyboardOptions = keyboardOptions,
            keyboardActions = keyboardActions,
            interactionSource = interaction,
            modifier = Modifier
                .fillMaxWidth()
                .semantics {
                    contentDescription = label
                    if (error != null) error(error)
                    if (autofill != null) contentType = autofill
                },
            decorationBox = { inner ->
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = MinTouchTarget)
                        .focusRing(focused, Shapes.pill, scheme.accent)
                        .background(scheme.card, Shapes.pill)
                        .border(1.dp, edge, Shapes.pill)
                        .padding(horizontal = Tokens.space4, vertical = Tokens.space2),
                    contentAlignment = Alignment.CenterStart,
                ) { inner() }
            },
        )
        if (error != null) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(Tokens.space1),
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.padding(start = Tokens.space2),
            ) {
                Icon(SquirrelIcon.Alert, contentDescription = null, tint = scheme.danger, size = Tokens.iconSm)
                BasicText(error, style = type.caption.copy(color = scheme.danger))
            }
        }
    }
}
