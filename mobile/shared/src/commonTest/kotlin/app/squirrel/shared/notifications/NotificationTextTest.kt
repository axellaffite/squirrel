package app.squirrel.shared.notifications

import app.squirrel.api.models.SharePermission
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class NotificationTextTest {
    @Test
    fun sharesSayWhatThePersonMayDo() {
        assertEquals("Anna shared “Trip” with you · can edit", notificationSentence(share("1")))
        assertEquals("shared “Trip” with you · can view", notificationAction(share("1", permission = SharePermission.read)))
        assertEquals("shared “Trip” with you", notificationAction(share("1", permission = null)))
    }

    @Test
    fun commentsQuoteTheirStart() {
        assertEquals("Ben commented on “beach.jpg”: Nice", notificationSentence(comment("1")))
        assertEquals("commented on “beach.jpg”", notificationAction(comment("1", excerpt = " ")))
    }

    @Test
    fun aDeletedAccountIsSomeone() {
        assertEquals("Someone shared “Trip” with you · can edit", notificationSentence(share("1").copy(actor = null)))
    }

    @Test
    fun osNotificationsPutTheEventInTheTitle() {
        val shared = osNotification(share("1"))
        assertEquals(OsNotification("1", "Anna shared “Trip” with you", "You can edit it."), shared)
        assertEquals(OsNotification("2", "Ben commented on “beach.jpg”", "Nice"), osNotification(comment("2")))
        assertNull(osNotification(comment("3", excerpt = null)).body)
    }

    @Test
    fun theSummaryCountsAndNamesTheNewest() {
        val summary = summaryNotification(listOf(comment("a"), share("b"), share("c"), share("d")))
        assertEquals(OsNotification(SUMMARY_KEY, "4 new notifications", "Ben commented on “beach.jpg”: Nice, and 3 more."), summary)
    }
}
