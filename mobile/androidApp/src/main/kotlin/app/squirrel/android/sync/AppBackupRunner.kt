package app.squirrel.android.sync

import app.squirrel.shared.sync.QueueState
import app.squirrel.shared.sync.StopCause
import app.squirrel.shared.sync.SyncState
import app.squirrel.ui.AppGraph
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.IO
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.withContext

/** The app's engine behind [BackupRunner]. */
class AppBackupRunner(
    private val graph: AppGraph,
    private val backup: AndroidBackup,
    private val scope: CoroutineScope,
) : BackupRunner {
    override suspend fun pending(): Pair<Long, Long> {
        graph.awaitReady()
        val counts = graph.queue.counts()
        val left = listOf(QueueState.New, QueueState.Hashed, QueueState.Missing, QueueState.Uploading).sumOf { counts[it] ?: 0L }
        return left to graph.queue.bytesLeft()
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    override val progress: Flow<SyncState> = graph.syncEngine.flatMapLatest { it?.state ?: flowOf(SyncState.Idle) }

    override suspend fun run(deadline: Long?): RunOutcome {
        graph.awaitReady()
        val engine = graph.syncEngine.value ?: return RunOutcome.NotSignedIn
        val summary = engine.runOnce(deadline)
        return when {
            summary.stoppedBy == StopCause.Deadline -> RunOutcome.Continue
            engine.state.value == SyncState.Error(app.squirrel.shared.sync.ErrorReason.Unreachable) -> RunOutcome.Retry
            engine.state.value == SyncState.Error(app.squirrel.shared.sync.ErrorReason.SignedOut) -> RunOutcome.NotSignedIn
            else -> RunOutcome.Done
        }
    }

    override suspend fun afterRun(outcome: RunOutcome?) {
        if (outcome == RunOutcome.NotSignedIn) return
        withContext(Dispatchers.IO) {
            val settings = graph.settings.backup()
            if (outcome == RunOutcome.Continue) backup.scheduler.continueLater(settings)
            backup.scheduler.rearm(settings)
        }
    }
}
