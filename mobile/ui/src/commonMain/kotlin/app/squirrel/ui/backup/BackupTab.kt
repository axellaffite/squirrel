package app.squirrel.ui.backup

import androidx.compose.runtime.Composable
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
    BackupScreen(vm, actions, onOpenSettings = {})
}
