package app.squirrel.ui.sharing

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.ShareDto
import app.squirrel.api.models.SharePermission
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.Card
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.SegmentedControl
import app.squirrel.theme.components.Sheet
import app.squirrel.theme.components.TextField
import androidx.compose.foundation.layout.Row
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow

private val PERMISSIONS = listOf(SharePermission.read to "Read", SharePermission.write to "Write")

/** The Share sheet ("Share “Holidays”"): add someone by username, change or stop each share. */
@Composable
fun ShareSheet(api: SquirrelApi, subject: Subject, name: String, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    val model = remember(subject) { ShareModel(api, subject, scope) }
    LaunchedEffect(model) { model.load() }
    val state by model.state.collectAsStateWithLifecycle()
    Sheet("Share “$name”", onDismiss = onClose) {
        TextField(
            value = state.username,
            onValueChange = model::setUsername,
            label = "Username",
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { model.share() }),
        )
        BasicText("Someone with an account on this Squirrel.", style = SquirrelTheme.type.caption)
        SegmentedControl(PERMISSIONS.map { it.second }, PERMISSIONS.indexOfFirst { it.first == state.permission }, onSelect = { model.setPermission(PERMISSIONS[it].first) })
        Button("Share", onClick = model::share, enabled = !state.busy, full = true, icon = SquirrelIcon.Share)
        state.error?.let { Alert(it, tone = AlertTone.Error) }
        val shares = state.shares
        when {
            shares == null -> if (state.error == null) Loading()
            shares.isEmpty() -> BasicText("Not shared with anyone yet.", style = SquirrelTheme.type.caption)
            else -> shares.forEach { share -> ShareRow(share, onChange = { model.change(share, it) }, onStop = { model.stop(share) }) }
        }
    }
}

/** One person with access: name, @username, Read/Write, and "Stop sharing with …". */
@Composable
private fun ShareRow(share: ShareDto, onChange: (SharePermission) -> Unit, onStop: () -> Unit) {
    val type = SquirrelTheme.type
    val name = share.person.displayName
    Card {
        Column(verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    BasicText(name, style = type.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    BasicText("@${share.person.username}", style = type.caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                IconButton(SquirrelIcon.X, label = "Stop sharing with $name", onClick = onStop)
            }
            SegmentedControl(
                PERMISSIONS.map { it.second },
                PERMISSIONS.indexOfFirst { it.first == share.permission },
                onSelect = { onChange(PERMISSIONS[it].first) },
            )
        }
    }
}
