package app.squirrel.ui.ios

import app.squirrel.shared.ios.background.BackgroundRunner
import app.squirrel.shared.ios.notify.IosNotificationPoster
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.StopCause
import app.squirrel.shared.sync.SyncState
import app.squirrel.ui.AppGraph
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch

/** The signed-in engine behind iOS background tasks (nothing when signed out). */
class GraphRunner(private val graph: () -> AppGraph) : BackgroundRunner {
    override suspend fun run(deadline: Long?, manual: Boolean, progress: (Long, Long) -> Unit) {
        val app = graph()
        app.awaitReady()
        val engine = app.syncEngine.value ?: return
        if (!manual && !app.settings.backup().autoBackup) return
        coroutineScope {
            val reporter = launch {
                engine.state.collect { state ->
                    if (state is SyncState.Uploading) progress(state.done.toLong(), state.of.toLong())
                }
            }
            try {
                if (manual) app.queue.retryFailed()
                var summary = engine.runOnce(deadline)
                if (summary.stoppedBy == StopCause.AlreadyRunning) {
                    engine.awaitIdle()
                    summary = engine.runOnce(deadline)
                }
            } finally {
                reporter.cancel()
            }
        }
    }

    override fun settings(): BackupSettings = graph().settings.backup()

    override suspend fun announce() {
        graph().announceNotifications(IosNotificationPoster())
    }
}
