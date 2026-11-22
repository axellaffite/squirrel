package app.squirrel.android.sync

import android.app.NotificationManager
import android.content.Context
import android.os.Build
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.ForegroundInfo
import androidx.work.WorkInfo
import androidx.work.WorkerParameters
import app.squirrel.shared.sync.SyncState
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
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
    /**
     * [manual]: "Back up now" (runs even with automatic backup off). [waitForNetwork]: a
     * foreground run, which waits out a network loss instead of ending.
     */
    suspend fun run(deadline: Long?, manual: Boolean, reconcile: Boolean = false, waitForNetwork: Boolean = false): RunOutcome
    /** After any run, before the worker returns: re-arm the content trigger; after a deadline: continue. */
    suspend fun afterRun(outcome: RunOutcome?, manual: Boolean)
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
/**
 * Android's ways of refusing a foreground promotion: ForegroundServiceStartNotAllowedException (an
 * IllegalStateException, Android 12+ from the background) and SecurityException (Android 14+, a
 * foreground-service type whose permission is missing).
 */
fun isForegroundRefusal(e: Throwable): Boolean = e is IllegalStateException || e is SecurityException

/** Stops that must not bring the run back by themselves. */
private val FINAL_STOPS = setOf(
    WorkInfo.STOP_REASON_NOT_STOPPED,
    // Automatic backup turned off, signed out.
    WorkInfo.STOP_REASON_CANCELLED_BY_APP,
    // The user stopped it (the notification's Stop, the task manager): they meant it.
    WorkInfo.STOP_REASON_USER,
    // Android 15's daily dataSync limit: a continuation would only run into it again.
    WorkInfo.STOP_REASON_FOREGROUND_SERVICE_TIMEOUT,
)

/**
 * Whether a run the system stopped ([stopReason], WorkInfo.STOP_REASON_*) should continue in a
 * follow-up: constraints, quotas, the device's state, but not [FINAL_STOPS]. WorkManager may also
 * reschedule it; the engine never runs twice at once.
 */
fun continuesAfterStop(stopReason: Int): Boolean = stopReason !in FINAL_STOPS

class BackupWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    private val notifications = ForegroundInfoFactory(context)

    override suspend fun getForegroundInfo(): ForegroundInfo = notifications.create(SyncState.Scanning)

    override suspend fun doWork(): Result {
        val runner = (applicationContext as? BackupHost)?.backupRunner ?: return Result.failure()
        var outcome: RunOutcome? = null
        val manual = inputData.getBoolean(KEY_MANUAL, false)
        return try {
            coroutineScope {
                val (assets, bytes) = runner.pending()
                // "Back up now" always: the user may leave the app and the run must go on.
                val foreground = (manual || assets > FOREGROUND_ASSETS || bytes > FOREGROUND_BYTES) && tryForeground()
                val progress = if (foreground) launch { showProgress(runner) } else null
                val deadline = if (foreground) null else System.currentTimeMillis() + DEADLINE_MS
                // The 6-hourly run also checks waiting assets against the phone (deleted ones).
                outcome = runner.run(
                    deadline, manual,
                    reconcile = tags.contains(BackupScheduler.PERIODIC_TAG),
                    // In the foreground, a VPN or Wi-Fi pausing with the screen off is waited out.
                    waitForNetwork = foreground,
                )
                progress?.cancel()
            }
            when (outcome) {
                RunOutcome.Retry -> Result.retry()
                else -> Result.success()
            }
        } catch (e: CancellationException) {
            Log.i(TAG, "Backup stopped by the system (reason $stopReasonText); it continues in a follow-up")
            // What was done is kept; a follow-up (promoted to the foreground when allowed) goes on.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && continuesAfterStop(stopReason)) outcome = RunOutcome.Continue
            throw e
        } catch (e: Exception) {
            Log.e(TAG, "Backup run failed", e)
            Result.failure()
        } finally {
            withContext(NonCancellable) { runner.afterRun(outcome, manual) }
        }
    }

    private suspend fun tryForeground(): Boolean = try {
        setForeground(notifications.create(SyncState.Scanning))
        true
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        if (!isForegroundRefusal(e)) throw e
        // Not allowed now (started from the background, missing FGS type permission): run with a deadline.
        Log.w(TAG, "Foreground not allowed: ${e.message}")
        false
    }

    private suspend fun showProgress(runner: BackupRunner) {
        val manager = applicationContext.getSystemService(NotificationManager::class.java)
        runner.progress.collect { state ->
            if (state is SyncState.Uploading || state is SyncState.Hashing || state is SyncState.WaitingForNetwork) {
                manager.notify(ForegroundInfoFactory.NOTIFICATION_ID, notifications.notification(state))
            }
        }
    }

    private val stopReasonText: String
        get() = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) stopReason.toString() else "?"

    companion object {
        private const val TAG = "SquirrelBackup"
        const val KEY_MANUAL = "manual"
        const val FOREGROUND_ASSETS = 20L
        const val FOREGROUND_BYTES = 200L * 1000 * 1000
        const val DEADLINE_MS = 9 * 60_000L
    }
}
