package app.squirrel.shared.ios.notify

import app.squirrel.shared.notifications.NotificationPoster
import app.squirrel.shared.notifications.OsNotification
import kotlinx.coroutines.suspendCancellableCoroutine
import platform.UserNotifications.UNAuthorizationOptionAlert
import platform.UserNotifications.UNAuthorizationOptionBadge
import platform.UserNotifications.UNAuthorizationOptionSound
import platform.UserNotifications.UNAuthorizationStatusAuthorized
import platform.UserNotifications.UNAuthorizationStatusEphemeral
import platform.UserNotifications.UNAuthorizationStatusProvisional
import platform.UserNotifications.UNMutableNotificationContent
import platform.UserNotifications.UNNotificationRequest
import platform.UserNotifications.UNNotificationSettings
import platform.UserNotifications.UNNotificationSound
import platform.UserNotifications.UNUserNotificationCenter
import kotlin.coroutines.resume

/** Shares and comments as local notifications; tapping one opens the app (the system's default). */
class IosNotificationPoster : NotificationPoster {
    private val center get() = UNUserNotificationCenter.currentNotificationCenter()

    override suspend fun enabled(): Boolean = suspendCancellableCoroutine { continuation ->
        center.getNotificationSettingsWithCompletionHandler { settings: UNNotificationSettings? ->
            val status = settings?.authorizationStatus
            continuation.resume(
                status == UNAuthorizationStatusAuthorized || status == UNAuthorizationStatusProvisional || status == UNAuthorizationStatusEphemeral,
            )
        }
    }

    override fun post(notifications: List<OsNotification>) {
        notifications.forEach { item ->
            val content = UNMutableNotificationContent().apply {
                setTitle(item.title)
                item.body?.let { setBody(it) }
                setSound(UNNotificationSound.defaultSound)
                setThreadIdentifier(THREAD)
            }
            // The same identifier replaces a notification still shown (the summary, a repeat).
            val request = UNNotificationRequest.requestWithIdentifier("$THREAD.${item.key}", content, trigger = null)
            center.addNotificationRequest(request) { error ->
                if (error != null) println("Squirrel: notification not posted: ${error.localizedDescription}")
            }
        }
    }

    private companion object {
        const val THREAD = "app.squirrel.activity"
    }
}

/** The system prompt to allow notifications (shown once; afterwards only Settings changes it). */
fun requestNotificationPermission() {
    UNUserNotificationCenter.currentNotificationCenter().requestAuthorizationWithOptions(
        UNAuthorizationOptionAlert or UNAuthorizationOptionBadge or UNAuthorizationOptionSound,
    ) { _, _ -> }
}
