package app.squirrel.shared.notifications

import app.squirrel.api.models.NotificationDto
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.time.Instant

class AnnouncerTest {
    private class Poster(var allowed: Boolean = true) : NotificationPoster {
        val posted = mutableListOf<OsNotification>()
        override suspend fun enabled() = allowed
        override fun post(notifications: List<OsNotification>) {
            posted += notifications
        }
    }

    private class Store(var mark: AnnounceMark? = null) : AnnounceStore {
        override suspend fun load() = mark
        override suspend fun save(mark: AnnounceMark) {
            this.mark = mark
        }
    }

    private var server = listOf<NotificationDto>()
    private val poster = Poster()
    private val store = Store()
    private val announcer = NotificationAnnouncer({ server }, store, poster)

    @Test
    fun announcesEachUnreadItemOnce() = runTest {
        server = listOf(share("b", "2026-11-24T10:05:00Z"), comment("a", "2026-11-24T10:00:00Z"), share("old", "2026-11-24T09:00:00Z", read = true))
        assertEquals(2, announcer.announce())
        // Oldest first, so the newest lands on top.
        assertEquals(listOf("a", "b"), poster.posted.map { it.key })
        assertEquals(AnnounceMark(Instant.parse("2026-11-24T10:05:00Z"), setOf("b")), store.mark)

        assertEquals(0, announcer.announce())
        assertEquals(2, poster.posted.size)

        server = listOf(comment("c", "2026-11-24T11:00:00Z")) + server
        assertEquals(1, announcer.announce())
        assertEquals("c", poster.posted.last().key)
    }

    @Test
    fun manyBecomeOneSummary() = runTest {
        server = (1..5).map { share("s$it", "2026-11-24T10:0$it:00Z") }.reversed()
        assertEquals(5, announcer.announce())
        assertEquals(listOf(SUMMARY_KEY), poster.posted.map { it.key })
        assertEquals("5 new notifications", poster.posted.single().title)
    }

    @Test
    fun nothingWhileNotificationsAreRefused() = runTest {
        poster.allowed = false
        server = listOf(share("a"))
        assertEquals(0, announcer.announce())
        assertNull(store.mark)
        // Allowed later: what is still unread is announced then.
        poster.allowed = true
        assertEquals(1, announcer.announce())
    }

    @Test
    fun itemsAtTheSameInstantAreToldApartById() {
        val at = "2026-11-24T10:00:00Z"
        val mark = advance(null, listOf(share("a", at)))
        assertEquals(listOf("b"), unannounced(listOf(share("b", at), share("a", at)), mark).map { it.id })
        assertEquals(setOf("a", "b"), advance(mark, listOf(share("b", at))).ids)
    }

    @Test
    fun olderItemsNeverMoveTheMarkBack() {
        val mark = AnnounceMark(Instant.parse("2026-11-24T10:00:00Z"), setOf("x"))
        assertEquals(mark, advance(mark, listOf(share("old", "2026-11-24T09:00:00Z"))))
        assertEquals(emptyList(), unannounced(listOf(share("old", "2026-11-24T09:00:00Z")), mark))
    }
}
