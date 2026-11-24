package app.squirrel.ui.backup

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import app.squirrel.ui.RegisterLsDo
import androidx.compose.ui.Modifier
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.shell.PageHeader
import androidx.compose.ui.text.input.ImeAction
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.TextField
import app.squirrel.theme.components.Toggle
import app.squirrel.theme.components.Well

/** Backup › Settings. */
@Composable
fun BackupSettingsScreen(vm: BackupSettingsViewModel, picker: AlbumPickerViewModel, onBack: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    val albums by picker.state.collectAsStateWithLifecycle()
    val type = SquirrelTheme.type
    SystemBack(onBack = onBack)
    RegisterLsDo("album_picker", remember(picker) { AlbumPickerLsDo(picker) })
    Column(
        modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(Tokens.space4),
    ) {
        PageHeader("Backup settings", onBack = onBack, backLabel = "Back to Backup")
        Well(verticalArrangement = Arrangement.spacedBy(Tokens.space1)) {
            val s = state.settings
            Toggle("Back up automatically", s.autoBackup, vm::setAutoBackup, description = "When new photos appear, and every few hours")
            Toggle("Only on Wi-Fi", s.wifiOnly, vm::setWifiOnly)
            Toggle("Only while charging", s.chargingOnly, vm::setChargingOnly)
            Toggle("Include videos", s.includeVideos, vm::setIncludeVideos)
        }
        Well {
            ListRow(albums.kind.title, icon = albums.kind.icon, meta = albums.label, onClick = picker::open)
        }
        Well(verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            BasicText("This phone", style = type.cardTitle)
            TextField(
                value = state.deviceName,
                onValueChange = vm::onNameChange,
                label = "Name",
                error = state.nameError,
                enabled = !state.renaming,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { vm.saveName() }),
            )
            BasicText(state.folderHint, style = type.caption)
            if (state.deviceName.trim() != state.savedName) {
                Button(if (state.renaming) "Saving…" else "Save name", onClick = vm::saveName, enabled = !state.renaming,
                    variant = ButtonVariant.Secondary)
            }
        }
        Spacer(Modifier.height(Tokens.railBarH))
    }
    AlbumPickerSheet(picker)
}
