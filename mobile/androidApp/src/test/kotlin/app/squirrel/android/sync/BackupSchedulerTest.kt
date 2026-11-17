package app.squirrel.android.sync

import android.app.Application
import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.work.Configuration
import androidx.work.NetworkType
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.testing.SynchronousExecutor
import androidx.work.testing.WorkManagerTestInitHelper
import app.squirrel.shared.settings.BackupSettings
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import kotlin.test.assertEquals
import kotlin.test.assertTrue

@RunWith(AndroidJUnit4::class)
@Config(sdk = [35], application = Application::class)
class BackupSchedulerTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private lateinit var work: WorkManager
    private lateinit var scheduler: BackupScheduler

    @Before
    fun setUp() {
        WorkManagerTestInitHelper.initializeTestWorkManager(
            context,
            Configuration.Builder().setExecutor(SynchronousExecutor()).build(),
        )
        work = WorkManager.getInstance(context)
        scheduler = BackupScheduler(context)
    }

    private fun info(name: String): WorkInfo? =
        work.getWorkInfosForUniqueWork(name).get().singleOrNull { !it.state.isFinished }

    @Test
    fun wifiOnlyWithoutChargingIsUnmeteredAndBatteryNotLow() {
        scheduler.apply(BackupSettings(wifiOnly = true, chargingOnly = false))

        val periodic = checkNotNull(info(BackupScheduler.PERIODIC))
        assertEquals(NetworkType.UNMETERED, periodic.constraints.requiredNetworkType)
        assertEquals(false, periodic.constraints.requiresCharging())
        assertTrue(periodic.constraints.requiresBatteryNotLow())
        assertEquals(6 * 3_600_000L, periodic.periodicityInfo?.repeatIntervalMillis)

        val onMedia = checkNotNull(info(BackupScheduler.ON_NEW_MEDIA))
        assertEquals(2, onMedia.constraints.contentUriTriggers.size)
        assertTrue(onMedia.constraints.contentUriTriggers.all { it.isTriggeredForDescendants })
    }

    @Test
    fun anyNetworkWhileCharging() {
        scheduler.apply(BackupSettings(wifiOnly = false, chargingOnly = true))

        val periodic = checkNotNull(info(BackupScheduler.PERIODIC))
        assertEquals(NetworkType.CONNECTED, periodic.constraints.requiredNetworkType)
        assertTrue(periodic.constraints.requiresCharging())
        assertTrue(checkNotNull(info(BackupScheduler.ON_NEW_MEDIA)).constraints.requiresCharging())
    }

    @Test
    fun changingSettingsUpdatesTheScheduledWork() {
        scheduler.apply(BackupSettings(wifiOnly = true))
        scheduler.apply(BackupSettings(wifiOnly = false))

        assertEquals(NetworkType.CONNECTED, checkNotNull(info(BackupScheduler.PERIODIC)).constraints.requiredNetworkType)
        assertEquals(NetworkType.CONNECTED, checkNotNull(info(BackupScheduler.ON_NEW_MEDIA)).constraints.requiredNetworkType)
    }

    @Test
    fun automaticBackupOffCancelsBackgroundWork() {
        scheduler.apply(BackupSettings())
        scheduler.apply(BackupSettings(autoBackup = false))

        assertEquals(null, info(BackupScheduler.PERIODIC))
        assertEquals(null, info(BackupScheduler.ON_NEW_MEDIA))
    }

    @Test
    fun backUpNowNeedsOnlyTheNetwork() {
        scheduler.backUpNow(BackupSettings(wifiOnly = true, chargingOnly = true))

        val now = checkNotNull(info(BackupScheduler.NOW))
        assertEquals(NetworkType.UNMETERED, now.constraints.requiredNetworkType)
        assertEquals(false, now.constraints.requiresCharging())
    }

    @Test
    fun signingOutCancelsEverything() {
        scheduler.apply(BackupSettings())
        scheduler.backUpNow(BackupSettings())
        scheduler.cancel()

        listOf(BackupScheduler.PERIODIC, BackupScheduler.ON_NEW_MEDIA, BackupScheduler.NOW).forEach {
            assertEquals(null, info(it), it)
        }
    }

    @Test
    fun reApplyingTheSameSettingsKeepsTheQueuedWork() {
        scheduler.apply(BackupSettings())
        val periodic = checkNotNull(info(BackupScheduler.PERIODIC)).id
        val onMedia = checkNotNull(info(BackupScheduler.ON_NEW_MEDIA)).id

        scheduler.apply(BackupSettings())

        assertEquals(periodic, checkNotNull(info(BackupScheduler.PERIODIC)).id)
        assertEquals(onMedia, checkNotNull(info(BackupScheduler.ON_NEW_MEDIA)).id)
    }

    @Test
    fun aConstraintChangeUpdatesTheTriggerInPlace() {
        scheduler.apply(BackupSettings(wifiOnly = true))
        val onMedia = checkNotNull(info(BackupScheduler.ON_NEW_MEDIA))

        scheduler.apply(BackupSettings(wifiOnly = false))

        val updated = checkNotNull(info(BackupScheduler.ON_NEW_MEDIA))
        assertEquals(onMedia.id, updated.id) // not cancelled and replaced
        assertEquals(NetworkType.CONNECTED, updated.constraints.requiredNetworkType)
    }
}
