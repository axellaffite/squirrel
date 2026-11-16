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
import androidx.work.WorkManager
import app.squirrel.shared.settings.BackupSettings
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

    fun apply(settings: BackupSettings) {
        if (!settings.autoBackup) {
            listOf(PERIODIC, ON_NEW_MEDIA).forEach(work::cancelUniqueWork)
            return
        }
        val periodic = PeriodicWorkRequestBuilder<BackupWorker>(PERIOD.inWholeHours, TimeUnit.HOURS, FLEX.inWholeHours, TimeUnit.HOURS)
            .setConstraints(constraints(settings))
            .build()
        work.enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.UPDATE, periodic)
        enqueueOnNewMedia(settings, ExistingWorkPolicy.REPLACE)
    }

    /** After each run: wait for the next MediaStore change again. */
    fun rearm(settings: BackupSettings) {
        if (settings.autoBackup) enqueueOnNewMedia(settings, ExistingWorkPolicy.KEEP)
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

    private fun enqueueOnNewMedia(settings: BackupSettings, policy: ExistingWorkPolicy) {
        val triggers = constraints(settings).let { base ->
            Constraints.Builder()
                .setRequiredNetworkType(base.requiredNetworkType)
                .setRequiresCharging(base.requiresCharging())
                .setRequiresBatteryNotLow(true)
                .addContentUriTrigger(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, true)
                .addContentUriTrigger(MediaStore.Video.Media.EXTERNAL_CONTENT_URI, true)
                .setTriggerContentUpdateDelay(UPDATE_DELAY.toJavaDuration())
                .setTriggerContentMaxDelay(MAX_DELAY.toJavaDuration())
                .build()
        }
        work.enqueueUniqueWork(ON_NEW_MEDIA, policy, OneTimeWorkRequestBuilder<BackupWorker>().setConstraints(triggers).build())
    }

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

        fun constraints(settings: BackupSettings): Constraints = Constraints.Builder()
            .setRequiredNetworkType(network(settings))
            .setRequiresCharging(settings.chargingOnly)
            .setRequiresBatteryNotLow(true)
            .build()
    }
}
