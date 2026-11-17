package app.squirrel.shared.ios.background

import app.squirrel.shared.settings.BackupSettings
import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import platform.BackgroundTasks.BGAppRefreshTaskRequest
import platform.BackgroundTasks.BGContinuedProcessingTask
import platform.BackgroundTasks.BGContinuedProcessingTaskRequest
import platform.BackgroundTasks.BGProcessingTaskRequest
import platform.BackgroundTasks.BGTask
import platform.BackgroundTasks.BGTaskRequest
import platform.BackgroundTasks.BGTaskScheduler
import platform.Foundation.NSDate
import platform.Foundation.NSOperatingSystemVersion
import platform.Foundation.NSProcessInfo
import platform.Foundation.dateWithTimeIntervalSinceNow
import platform.UIKit.UIApplication
import platform.UIKit.UIBackgroundTaskIdentifier
import platform.UIKit.UIBackgroundTaskInvalid
import kotlinx.cinterop.useContents
import kotlinx.cinterop.cValue

const val PROCESSING_TASK = "app.squirrel.backup.processing"
const val REFRESH_TASK = "app.squirrel.backup.refresh"

/** Continued processing ids must start with the bundle id (`app.squirrel.ios`). */
const val NOW_TASK = "app.squirrel.ios.backup.now"

private const val PROCESSING_AFTER_S = 60.0 * 60
private const val REFRESH_AFTER_S = 6 * 60.0 * 60

/** A refresh task gets about 30 s: the run stops a little before. */
private const val REFRESH_BUDGET_MS = 25_000L

/** What a background task runs: the engine, until [deadline] (epoch ms) or cancelled. */
interface BackgroundRunner {
    /** One run; [progress] gets (done, total) when known. */
    @Throws(Exception::class)
    suspend fun run(deadline: Long?, manual: Boolean, progress: (Long, Long) -> Unit = { _, _ -> })

    /** The current settings (read from the database). */
    fun settings(): BackupSettings
}

/**
 * Background backups through `BGTaskScheduler`: a processing task (hours apart, may need power),
 * an app refresh task (a short run), and on iOS 26+ "Back up now" as a continued processing task
 * the user sees progress for. Handlers are registered at launch, before it finishes.
 */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class BackgroundTasks(private val runner: BackgroundRunner, private val scope: CoroutineScope, private val now: () -> Long) {
    private val scheduler get() = BGTaskScheduler.sharedScheduler

    fun register() {
        scheduler.registerForTaskWithIdentifier(PROCESSING_TASK, usingQueue = null) { task -> handle(task!!, deadline = null) }
        scheduler.registerForTaskWithIdentifier(REFRESH_TASK, usingQueue = null) { task -> handle(task!!, deadline = now() + REFRESH_BUDGET_MS) }
        if (continuedProcessing()) {
            scheduler.registerForTaskWithIdentifier(NOW_TASK, usingQueue = null) { task -> handleNow(task as BGContinuedProcessingTask) }
        }
    }

    /** Submits (or replaces) the periodic requests; none while automatic backup is off. */
    fun schedule(settings: BackupSettings) {
        scheduler.cancelTaskRequestWithIdentifier(PROCESSING_TASK)
        scheduler.cancelTaskRequestWithIdentifier(REFRESH_TASK)
        if (!settings.autoBackup) return
        submit(BGProcessingTaskRequest(PROCESSING_TASK).apply {
            requiresNetworkConnectivity = true
            requiresExternalPower = settings.chargingOnly
            earliestBeginDate = NSDate.dateWithTimeIntervalSinceNow(PROCESSING_AFTER_S)
        })
        submit(BGAppRefreshTaskRequest(REFRESH_TASK).apply {
            earliestBeginDate = NSDate.dateWithTimeIntervalSinceNow(REFRESH_AFTER_S)
        })
    }

    /**
     * "Back up now": a continued processing task on iOS 26+ (progress in the system UI, carries on
     * in the background); otherwise, or if the system refuses it, a run right here that asks for
     * extra time when the app leaves the screen. Queued chunks go on in the background session.
     */
    fun backUpNow() {
        if (continuedProcessing()) {
            val request = BGContinuedProcessingTaskRequest(NOW_TASK, title = "Backing up", subtitle = "Squirrel")
            if (submit(request)) return
        }
        runHere(manual = true)
    }

    /** New photos while the app is open: a run now, if automatic backup is on. */
    fun runSoon() {
        if (runner.settings().autoBackup) runHere(manual = false)
    }

    fun cancel() = scheduler.cancelAllTaskRequests()

    private fun handle(task: BGTask, deadline: Long?) {
        val job = scope.launch {
            try {
                runner.run(deadline, manual = false)
            } finally {
                schedule(runner.settings())
            }
        }
        task.expirationHandler = { job.cancel() }
        job.invokeOnCompletion { task.setTaskCompletedWithSuccess(it == null) }
    }

    private fun handleNow(task: BGContinuedProcessingTask) {
        val job = scope.launch {
            runner.run(deadline = null, manual = true) { done, total ->
                task.progress.totalUnitCount = total
                task.progress.completedUnitCount = done
                task.updateTitle("Backing up", subtitle = "$done of $total")
            }
        }
        task.expirationHandler = { job.cancel() }
        job.invokeOnCompletion { task.setTaskCompletedWithSuccess(it == null) }
    }

    private fun runHere(manual: Boolean) {
        var extra: UIBackgroundTaskIdentifier = UIBackgroundTaskInvalid
        var job: Job? = null
        extra = UIApplication.sharedApplication.beginBackgroundTaskWithExpirationHandler { job?.cancel() }
        job = scope.launch {
            try {
                runner.run(deadline = null, manual = manual)
            } finally {
                if (extra != UIBackgroundTaskInvalid) UIApplication.sharedApplication.endBackgroundTask(extra)
            }
        }
    }

    /** False on the simulator for periodic tasks (it does not run them) and when refused. */
    private fun submit(request: BGTaskRequest): Boolean {
        val accepted = scheduler.submitTaskRequest(request, null)
        // Never NSLog with Kotlin vararg objects: Kotlin/Native does not bridge variadic arguments.
        if (!accepted) println("Squirrel: background task ${request.identifier} not accepted")
        return accepted
    }

    private fun continuedProcessing(): Boolean =
        NSProcessInfo.processInfo.isOperatingSystemAtLeastVersion(cValue<NSOperatingSystemVersion> { majorVersion = 26; minorVersion = 0; patchVersion = 0 })
}
