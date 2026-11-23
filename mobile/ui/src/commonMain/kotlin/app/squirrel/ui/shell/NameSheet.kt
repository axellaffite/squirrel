package app.squirrel.ui.shell

import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.text.input.ImeAction
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.Sheet
import app.squirrel.theme.components.TextField
import kotlinx.coroutines.launch

/**
 * The web's NameDialog as a sheet: one "Name" field, checked by [validate] first, then sent with
 * [onSubmit], which answers an error message or null once done (the sheet then closes).
 */
@Composable
fun NameSheet(
    title: String,
    submitLabel: String,
    initial: String = "",
    validate: (String) -> String?,
    onSubmit: suspend (String) -> String?,
    onClose: () -> Unit,
) {
    var name by remember { mutableStateOf(initial) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val submit = {
        error = validate(name)
        if (error == null && !busy) {
            busy = true
            scope.launch {
                val problem = try {
                    onSubmit(name.trim())
                } finally {
                    busy = false
                }
                if (problem == null) onClose() else error = problem
            }
        }
    }
    Sheet(title, onDismiss = onClose) {
        TextField(
            value = name,
            onValueChange = {
                name = it
                error = null
            },
            label = "Name",
            error = error,
            keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { submit() }),
        )
        Button(submitLabel, onClick = { submit() }, enabled = !busy, full = true)
        Button("Cancel", onClick = onClose, variant = ButtonVariant.Quiet, full = true)
    }
}
