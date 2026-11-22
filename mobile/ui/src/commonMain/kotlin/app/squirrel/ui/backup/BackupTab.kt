package app.squirrel.ui.backup

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import app.squirrel.api.models.RenameDeviceRequest
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.AppState
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.ui.AppGraph
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.withContext
import kotlin.time.Clock

/** The Backup tab of the shell. */
@Composable
fun BackupTab(graph: AppGraph, actions: PhotoAccessActions) {
    val vm = viewModel {
        BackupViewModel(
            status = graph.backupStatus(),
            settings = graph.settings.backupFlow(),
            locationsIncluded = { graph.backup?.locationsIncluded() ?: true },
            access = { graph.backup?.source?.access() ?: MediaAccess.Full },
            now = { Clock.System.now().toEpochMilliseconds() },
            onBackUpNow = graph::backUpNow,
            failures = { withContext(Dispatchers.IO) { graph.queue.failures() } },
        )
    }
    // One choice of albums or folders behind both the Backup screen and its settings.
    val picker = viewModel(key = "backup-albums") {
        AlbumPickerViewModel(
            store = graph.settings,
            applySchedule = graph::applySchedule,
            loadAlbums = { graph.backup?.albums().orEmpty() },
            kind = graph.backup?.albumKind ?: AlbumKind.Albums,
        )
    }
    var settingsOpen by rememberSaveable { mutableStateOf(false) }
    if (settingsOpen) {
        val settingsVm = viewModel {
            BackupSettingsViewModel(
                store = graph.settings,
                applySchedule = graph::applySchedule,
                rename = { name ->
                    val api = checkNotNull(graph.api) { "Signed out" }
                    val kept = api.devices.renameCurrentDevice(RenameDeviceRequest(name)).bodyOrThrow().device.name
                    graph.refreshAccount()
                    kept
                },
                deviceName = (graph.state.value as? AppState.SignedIn)?.account?.deviceName.orEmpty(),
            )
        }
        BackupSettingsScreen(settingsVm, picker, onBack = { settingsOpen = false })
    } else {
        BackupScreen(vm, picker, actions, onOpenSettings = { settingsOpen = true })
    }
}
