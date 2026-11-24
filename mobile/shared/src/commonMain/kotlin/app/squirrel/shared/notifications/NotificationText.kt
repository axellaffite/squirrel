package app.squirrel.shared.notifications

import app.squirrel.api.models.NotificationDto
import app.squirrel.api.models.NotificationKind
import app.squirrel.api.models.SharePermission

/** Who shared or commented; their account may have been deleted since. */
fun actorName(item: NotificationDto): String = item.actor?.displayName ?: "Someone"

/** What happened, after the actor's name: "shared “Trip” with you · can edit", "commented on “beach.jpg”: Nice". */
fun notificationAction(item: NotificationDto): String {
    val name = "“${item.subject.name}”"
    return when (item.kind) {
        NotificationKind.share -> "shared $name with you" + when (item.permission) {
            SharePermission.write -> " · can edit"
            SharePermission.read -> " · can view"
            else -> ""
        }
        NotificationKind.comment -> "commented on $name" + (item.excerpt?.takeIf { it.isNotBlank() }?.let { ": $it" } ?: "")
        else -> "has news about $name"
    }
}

/** The whole line: "Anna shared “Trip” with you · can edit". */
fun notificationSentence(item: NotificationDto): String = "${actorName(item)} ${notificationAction(item)}"

/** One OS notification: [key] identifies it (a notification id, or [SUMMARY_KEY]). */
data class OsNotification(val key: String, val title: String, val body: String?)

const val SUMMARY_KEY = "summary"

/** The OS notification for one item: the event as the title, the permission or comment below. */
fun osNotification(item: NotificationDto): OsNotification {
    val actor = actorName(item)
    val name = "“${item.subject.name}”"
    return when (item.kind) {
        NotificationKind.share -> OsNotification(
            item.id,
            "$actor shared $name with you",
            when (item.permission) {
                SharePermission.write -> "You can edit it."
                SharePermission.read -> "You can view it."
                else -> null
            },
        )
        NotificationKind.comment -> OsNotification(item.id, "$actor commented on $name", item.excerpt?.takeIf { it.isNotBlank() })
        else -> OsNotification(item.id, notificationSentence(item), null)
    }
}

/** One notification for many new items (newest first): their count, and the newest. */
fun summaryNotification(items: List<NotificationDto>): OsNotification =
    OsNotification(SUMMARY_KEY, "${items.size} new notifications", "${notificationSentence(items.first())}, and ${items.size - 1} more.")
