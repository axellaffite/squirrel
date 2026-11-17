package app.squirrel.android.sync

import android.content.Context
import android.provider.MediaStore
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkInfo
import androidx.work.WorkManager
import app.squirrel.shared.settings.BackupSettings
import java.util.UUID
import java.util.concurrent.TimeUnit
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlin.time.toJavaDuration

/**
 * Background backups: a periodic run every 6 h, a run when MediaStore changes (content-URI
 * trigger, works with the app closed, re-armed after each run), "Back up now" as expedited work,
 * and a continuation when a run hit its deadline. Constraints follow the backup settings.
 */
class BackupScheduler(context: Context) {
    private val work = WorkManager.getInstance(context.applicationContext)

    /**
     * Brings the background work in line with [settings] without ever cancelling a run: queued
     * work is kept, or updated in place when its constraints changed; running work is left alone
     * (it re-arms with the current settings when it finishes).
     */
    fun apply(settings: BackupSettings) {
        if (!settings.autoBackup) {
            listOf(PERIODIC, ON_NEW_MEDIA).forEach(work::cancelUniqueWork)
            return
        }
        val periodic = infos(PERIODIC).firstOrNull { !it.state.isFinished }
        if (periodic == null || periodic.constraints != constraints(settings)) {
            // UPDATE keeps a running period running; the new constraints apply from the next one.
            val request = PeriodicWorkRequestBuilder<BackupWorker>(PERIOD.inWholeHours, TimeUnit.HOURS, FLEX.inWholeHours, TimeUnit.HOURS)
                .setConstraints(constraints(settings))
                .build()
            work.enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.UPDATE, request)
        }
        ensureOnNewMedia(settings)
    }

    /**
     * After every run, whichever work it was: wait for the next MediaStore change again. Called
     * from inside the trigger's own run, where KEEP would see that run and do nothing, so a fresh
     * trigger is appended behind it (armed as soon as this run ends).
     */
    fun rearm(settings: BackupSettings) {
        if (!settings.autoBackup) return
        val existing = infos(ON_NEW_MEDIA)
        val pending = existing.any { it.state == WorkInfo.State.ENQUEUED || it.state == WorkInfo.State.BLOCKED }
        if (!pending && existing.any { it.state == WorkInfo.State.RUNNING }) {
            work.enqueueUniqueWork(ON_NEW_MEDIA, ExistingWorkPolicy.APPEND_OR_REPLACE, onNewMediaRequest(settings))
        } else {
            ensureOnNewMedia(settings)
        }
    }

    /** User-initiated: expedited when the quota allows. Expedited work takes network constraints only. */
    fun backUpNow(settings: BackupSettings) {
        val request = OneTimeWorkRequestBuilder<BackupWorker>()
            .setConstraints(Constraints.Builder().setRequiredNetworkType(network(settings)).build())
            .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
            .build()
        work.enqueueUniqueWork(NOW, ExistingWorkPolicy.KEEP, request)
    }

    /** New photos while the app is open (ContentObserver): a prompt run. */
    fun onNewMedia(settings: BackupSettings) {
        if (!settings.autoBackup) return
        val request = OneTimeWorkRequestBuilder<BackupWorker>().setConstraints(constraints(settings)).build()
        work.enqueueUniqueWork(ON_CHANGE, ExistingWorkPolicy.KEEP, request)
    }

    /** A run stopped at its deadline with work left: continue as soon as allowed. */
    fun continueLater(settings: BackupSettings) {
        val request = OneTimeWorkRequestBuilder<BackupWorker>().setConstraints(constraints(settings)).build()
        work.enqueueUniqueWork(CONTINUE, ExistingWorkPolicy.REPLACE, request)
    }

    fun cancel() {
        listOf(PERIODIC, ON_NEW_MEDIA, NOW, ON_CHANGE, CONTINUE).forEach(work::cancelUniqueWork)
    }

    /** Armed once: kept when queued with the same constraints, updated in place when not. */
    private fun ensureOnNewMedia(settings: BackupSettings) {
        val existing = infos(ON_NEW_MEDIA)
        val pending = existing.firstOrNull { it.state == WorkInfo.State.ENQUEUED || it.state == WorkInfo.State.BLOCKED }
        when {
            pending != null -> if (pending.constraints != triggerConstraints(settings)) {
                work.updateWork(onNewMediaRequest(settings, pending.id))
            }
            existing.any { it.state == WorkInfo.State.RUNNING } -> Unit // it re-arms when it finishes
            else -> work.enqueueUniqueWork(ON_NEW_MEDIA, ExistingWorkPolicy.KEEP, onNewMediaRequest(settings))
        }
    }

    private fun onNewMediaRequest(settings: BackupSettings, id: UUID? = null) =
        OneTimeWorkRequestBuilder<BackupWorker>()
            .setConstraints(triggerConstraints(settings))
            .apply { if (id != null) setId(id) }
            .build()

    private fun infos(name: String): List<WorkInfo> = work.getWorkInfosForUniqueWork(name).get()

    companion object {
        const val PERIODIC = "backup-periodic"
        const val ON_NEW_MEDIA = "backup-on-new-media"
        const val ON_CHANGE = "backup-on-change"
        const val NOW = "backup-now"
        const val CONTINUE = "backup-continue"
        private val PERIOD = 6.hours
        private val FLEX = 1.hours
        private val UPDATE_DELAY = 30.seconds
        private val MAX_DELAY = 10.minutes

        fun network(settings: BackupSettings): NetworkType =
            if (settings.wifiOnly) NetworkType.UNMETERED else NetworkType.CONNECTED

        fun triggerConstraints(settings: BackupSettings): Constraints = Constraints.Builder()
            .setRequiredNetworkType(network(settings))
            .setRequiresCharging(settings.chargingOnly)
            .setRequiresBatteryNotLow(true)
            .addContentUriTrigger(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, true)
            .addContentUriTrigger(MediaStore.Video.Media.EXTERNAL_CONTENT_URI, true)
            .setTriggerContentUpdateDelay(UPDATE_DELAY.toJavaDuration())
            .setTriggerContentMaxDelay(MAX_DELAY.toJavaDuration())
            .build()

        fun constraints(settings: BackupSettings): Constraints = Constraints.Builder()
            .setRequiredNetworkType(network(settings))
            .setRequiresCharging(settings.chargingOnly)
            .setRequiresBatteryNotLow(true)
            .build()
    }
}
