package app.squirrel.android.notify

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import app.squirrel.android.MainActivity
import app.squirrel.android.R
import app.squirrel.shared.notifications.NotificationPoster
import app.squirrel.shared.notifications.OsNotification
import app.squirrel.shared.notifications.SUMMARY_KEY

/** Shares and comments as Android notifications, on their own channel; tapping one opens the app. */
class AndroidNotificationPoster(private val context: Context) : NotificationPoster {
    private val manager = NotificationManagerCompat.from(context)

    /** Android 13+: also false until POST_NOTIFICATIONS is granted. */
    override suspend fun enabled(): Boolean = allowed()

    private fun allowed(): Boolean = manager.areNotificationsEnabled() &&
        (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED)

    override fun post(notifications: List<OsNotification>) {
        if (!allowed()) return
        ensureChannel()
        notifications.forEach { item ->
            val notification = NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(android.R.drawable.stat_notify_chat)
                .setContentTitle(item.title)
                .apply { item.body?.let { setContentText(it).setStyle(NotificationCompat.BigTextStyle().bigText(it)) } }
                .setCategory(NotificationCompat.CATEGORY_SOCIAL)
                .setContentIntent(openApp())
                .setAutoCancel(true)
                .build()
            try {
                manager.notify(idOf(item.key), notification)
            } catch (_: SecurityException) {
                return // The permission was withdrawn in between.
            }
        }
    }

    private fun openApp(): PendingIntent {
        val intent = Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        return PendingIntent.getActivity(context, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    private fun ensureChannel() {
        val system = context.getSystemService(NotificationManager::class.java)
        if (system.getNotificationChannel(CHANNEL) == null) {
            system.createNotificationChannel(NotificationChannel(CHANNEL, context.getString(R.string.activity_channel), NotificationManager.IMPORTANCE_DEFAULT))
        }
    }

    companion object {
        const val CHANNEL = "activity"
        private const val SUMMARY_ID = 2001

        /** One Android id per notification (the backup's progress is 1001). */
        fun idOf(key: String): Int = if (key == SUMMARY_KEY) SUMMARY_ID else key.hashCode().let { if (it == SUMMARY_ID || it == 1001) it + 1 else it }
    }
}
