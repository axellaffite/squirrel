package app.squirrel.android.sync

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.ServiceInfo
import androidx.core.app.NotificationCompat
import androidx.work.ForegroundInfo
import app.squirrel.shared.sync.SyncState
import app.squirrel.shared.sync.formatCount

/** The `dataSync` notification of a long backup ("Backing up 12 of 340"). */
class ForegroundInfoFactory(private val context: Context) {
    fun create(state: SyncState): ForegroundInfo {
        ensureChannel()
        return ForegroundInfo(NOTIFICATION_ID, notification(state), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
    }

    fun notification(state: SyncState): Notification {
        val builder = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_upload)
            .setContentTitle("Backing up your photos")
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
        when (state) {
            is SyncState.Uploading -> builder
                .setContentText("Backing up ${formatCount(state.done + 1L)} of ${formatCount(state.of.toLong())}")
                .setProgress(state.of, state.done, false)
            is SyncState.Hashing -> builder.setContentText("Preparing ${formatCount(state.of.toLong())} items").setProgress(0, 0, true)
            else -> builder.setContentText("Checking what is new").setProgress(0, 0, true)
        }
        return builder.build()
    }

    private fun ensureChannel() {
        val manager = context.getSystemService(NotificationManager::class.java)
        if (manager.getNotificationChannel(CHANNEL) == null) {
            manager.createNotificationChannel(NotificationChannel(CHANNEL, "Backup", NotificationManager.IMPORTANCE_LOW))
        }
    }

    companion object {
        const val CHANNEL = "backup"
        const val NOTIFICATION_ID = 1001
    }
}
