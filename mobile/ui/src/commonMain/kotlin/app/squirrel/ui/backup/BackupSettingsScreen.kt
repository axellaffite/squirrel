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
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.Icon
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.Sheet
import app.squirrel.theme.components.TextField
import app.squirrel.theme.components.Toggle
import app.squirrel.theme.components.Well
import app.squirrel.theme.tabular

/** Backup › Settings. */
@Composable
fun BackupSettingsScreen(vm: BackupSettingsViewModel, onBack: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    val type = SquirrelTheme.type
    Column(
        modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(Tokens.space4),
    ) {
        Button("Backup", onClick = onBack, variant = ButtonVariant.Quiet, size = ButtonSize.Sm, icon = SquirrelIcon.ChevronLeft)
        Well(verticalArrangement = Arrangement.spacedBy(Tokens.space1)) {
            val s = state.settings
            Toggle("Back up automatically", s.autoBackup, vm::setAutoBackup, description = "When new photos appear, and every few hours")
            Toggle("Only on Wi-Fi", s.wifiOnly, vm::setWifiOnly)
            Toggle("Only while charging", s.chargingOnly, vm::setChargingOnly)
            Toggle("Include videos", s.includeVideos, vm::setIncludeVideos)
        }
        Well {
            ListRow("Albums", icon = SquirrelIcon.Library, meta = state.albumsLabel, onClick = vm::openAlbums)
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
    if (state.albumsOpen) {
        Sheet("Albums", onDismiss = vm::closeAlbums) {
            val filter = state.settings.albums
            ListRow("All photos", icon = SquirrelIcon.Images, onClick = vm::selectAllAlbums, trailing = {
                if (filter == AlbumFilter.All) Icon(SquirrelIcon.Check, contentDescription = "Selected", tint = SquirrelTheme.scheme.accent)
            })
            val albums = state.albums
            if (albums == null) {
                BasicText("Loading albums…", style = type.caption)
            } else {
                albums.forEach { (name, count) ->
                    val selected = filter is AlbumFilter.Selected && name in filter.names
                    ListRow(name, icon = SquirrelIcon.Folder, meta = count.toString(), onClick = { vm.toggleAlbum(name) }, trailing = {
                        if (selected) Icon(SquirrelIcon.Check, contentDescription = "Selected", tint = SquirrelTheme.scheme.accent)
                    })
                }
            }
            BasicText("Changing albums checks your whole library again; nothing is deleted from your Squirrel.",
                style = type.caption.tabular())
        }
    }
}
