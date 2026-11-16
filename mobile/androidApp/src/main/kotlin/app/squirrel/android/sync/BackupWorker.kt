package app.squirrel.android.sync

import android.app.NotificationManager
import android.content.Context
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.ForegroundInfo
import androidx.work.WorkerParameters
import app.squirrel.shared.sync.SyncState
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

/** How a backup run ended, as the worker needs to know it. */
enum class RunOutcome { Done, Continue, Retry, NotSignedIn }

/** The backup the worker drives (the app's engine; a fake in tests). */
interface BackupRunner {
    /** Assets and bytes left, to decide on a foreground run. */
    suspend fun pending(): Pair<Long, Long>
    val progress: Flow<SyncState>
    suspend fun run(deadline: Long?): RunOutcome
    /** After any run: re-arm the content trigger; after a deadline: continue. */
    fun afterRun(outcome: RunOutcome?)
}

/** Implemented by the Application: where workers find the runner. */
interface BackupHost {
    val backupRunner: BackupRunner
}

/**
 * One backup run. Long first backups (more than 20 assets or 200 MB) run in the foreground with a
 * `dataSync` notification and no deadline; others stop after 9 minutes (WorkManager's 10-minute
 * limit) and continue in a follow-up. Stops by the system (constraints, quotas on Android 16,
 * Android 15's 6-hour dataSync limit) cancel the run; what was done is kept and WorkManager
 * reschedules. Paused runs succeed: the constraints re-trigger them.
 */
class BackupWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    private val notifications = ForegroundInfoFactory(context)

    override suspend fun getForegroundInfo(): ForegroundInfo = notifications.create(SyncState.Scanning)

    override suspend fun doWork(): Result {
        val runner = (applicationContext as? BackupHost)?.backupRunner ?: return Result.failure()
        var outcome: RunOutcome? = null
        return try {
            coroutineScope {
                val (assets, bytes) = runner.pending()
                val foreground = (assets > FOREGROUND_ASSETS || bytes > FOREGROUND_BYTES) && tryForeground()
                val progress = if (foreground) launch { showProgress(runner) } else null
                val deadline = if (foreground) null else System.currentTimeMillis() + DEADLINE_MS
                outcome = runner.run(deadline)
                progress?.cancel()
            }
            when (outcome) {
                RunOutcome.Retry -> Result.retry()
                else -> Result.success()
            }
        } catch (e: CancellationException) {
            Log.i(TAG, "Backup stopped by the system (reason $stopReasonText); it resumes on the next run")
            throw e
        } catch (e: Exception) {
            Log.e(TAG, "Backup run failed", e)
            Result.failure()
        } finally {
            runner.afterRun(outcome)
        }
    }

    private suspend fun tryForeground(): Boolean = try {
        setForeground(notifications.create(SyncState.Scanning))
        true
    } catch (e: IllegalStateException) {
        // Not allowed from the background on this Android version: run with a deadline instead.
        Log.w(TAG, "Foreground not allowed: ${e.message}")
        false
    }

    private suspend fun showProgress(runner: BackupRunner) {
        val manager = applicationContext.getSystemService(NotificationManager::class.java)
        runner.progress.collect { state ->
            if (state is SyncState.Uploading || state is SyncState.Hashing) {
                manager.notify(ForegroundInfoFactory.NOTIFICATION_ID, notifications.notification(state))
            }
        }
    }

    private val stopReasonText: String
        get() = if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S) stopReason.toString() else "?"

    companion object {
        private const val TAG = "SquirrelBackup"
        const val FOREGROUND_ASSETS = 20L
        const val FOREGROUND_BYTES = 200L * 1000 * 1000
        const val DEADLINE_MS = 9 * 60_000L
    }
}
