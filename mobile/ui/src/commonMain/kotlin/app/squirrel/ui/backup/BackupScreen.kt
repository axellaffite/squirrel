package app.squirrel.ui.backup

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.shared.sync.QueuedAsset
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.GraphiteCard
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.Progress
import app.squirrel.theme.components.Sheet
import app.squirrel.theme.components.Well
import app.squirrel.theme.tabular
import app.squirrel.ui.shell.PageHeader
import kotlinx.coroutines.launch

/** MASTER.md phone section: the Backup screen with its one graphite status card. */
@Composable
fun BackupScreen(vm: BackupViewModel, actions: PhotoAccessActions, onOpenSettings: () -> Unit) {
    val ui by vm.ui.collectAsStateWithLifecycle()
    val failed by vm.failed.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    LifecycleResumeEffect(Unit) {
        vm.onResume()
        onPauseOrDispose { }
    }
    val state = ui ?: return
    Column(
        modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(Tokens.space4),
    ) {
        PageHeader("Backup")
        if (state.access == AccessUi.None) {
            EmptyState(SquirrelIcon.Images, "Allow access to your photos", "Squirrel backs up the photos and videos on this phone.")
            Button("Allow access", onClick = actions::requestAccess, full = true, size = ButtonSize.Lg)
            return@Column
        }
        StatusCard(state)
        Button(state.button, onClick = vm::backUpNow, enabled = !state.running, full = true, size = ButtonSize.Lg)
        state.notice?.let { Alert(it, tone = if (state.noticeIsError) AlertTone.Error else AlertTone.Info) }
        (state.access as? AccessUi.Partial)?.let { partial ->
            Alert(partial.text) {
                Button("Allow all photos", onClick = actions::allowAll, variant = ButtonVariant.Secondary, size = ButtonSize.Sm)
                Button("Change selection", onClick = actions::changeSelection, variant = ButtonVariant.Quiet, size = ButtonSize.Sm)
            }
        }
        state.locationWarning?.let { Alert(it) }
        Well(verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            ListRow("Backed up", icon = SquirrelIcon.CheckCircle, meta = state.backedUp)
            ListRow("Waiting", icon = SquirrelIcon.Upload, meta = state.waiting)
            ListRow(
                "Failed",
                icon = SquirrelIcon.Alert,
                meta = state.failed,
                onClick = if (state.failedCount > 0) ({ scope.launch { vm.showFailures() } }) else null,
            )
            ListRow("Backup settings", icon = SquirrelIcon.More, onClick = onOpenSettings)
        }
        Spacer(Modifier.height(Tokens.railBarH))
    }
    failed?.let { rows -> FailureSheet(rows, onRetry = vm::retryFailures, onDismiss = vm::hideFailures) }
}

@Composable
private fun StatusCard(state: BackupUi) {
    val scheme = SquirrelTheme.scheme
    val type = SquirrelTheme.type
    GraphiteCard {
        BasicText(state.featured, style = type.display.copy(color = scheme.railInk).tabular())
        BasicText(state.detail, style = type.caption.copy(color = scheme.railMuted).tabular())
        if (state.running) {
            state.progress?.let { Progress(it, label = state.progressLabel ?: "Backup progress") }
            state.progressLabel?.let { BasicText(it, style = type.caption.copy(color = scheme.railMuted), maxLines = 1) }
        } else {
            BasicText(state.lastRun, style = type.caption.copy(color = scheme.railMuted))
        }
    }
}

@Composable
private fun FailureSheet(rows: List<QueuedAsset>, onRetry: () -> Unit, onDismiss: () -> Unit) {
    Sheet("Not backed up", onDismiss = onDismiss) {
        rows.forEach { ListRow(it.name, icon = SquirrelIcon.Alert, meta = it.lastError ?: "Unknown error") }
        Button("Try again", onClick = onRetry, full = true)
    }
}
