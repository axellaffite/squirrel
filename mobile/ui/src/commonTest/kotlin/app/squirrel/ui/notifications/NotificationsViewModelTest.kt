package app.squirrel.ui.notifications

import app.squirrel.api.models.NotificationSubjectDto
import app.squirrel.api.models.SubjectKind
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class NotificationsViewModelTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun theBellCountsAndTheSheetPagesThroughTheList() = runReal {
        val vm = NotificationsViewModel(NotificationsServer().api())
        vm.resumed()
        assertEquals(4, vm.state.await { it.unread == 4L }.unread)
        assertNull(vm.state.value.items, "the list loads only when the sheet opens")

        vm.open()
        val first = vm.state.await { it.items != null && !it.loading }
        assertTrue(first.open)
        assertEquals(listOf("n1", "n2"), first.items.orEmpty().map { it.id })
        assertEquals("c2", first.cursor)

        vm.more()
        val all = vm.state.await { it.cursor == null && !it.loadingMore }
        assertEquals(listOf("n1", "n2", "n3", "n4"), all.items.orEmpty().map { it.id })
    }

    @Test
    fun tappingMarksReadClosesAndSaysWhereToGo() = runReal {
        val server = NotificationsServer()
        val vm = NotificationsViewModel(server.api())
        vm.open()
        vm.state.await { it.items != null && !it.loading && it.unread == 4L }

        val went = mutableListOf<NotificationTarget>()
        assertTrue(vm.select("n2") { went += it })
        assertEquals(listOf<NotificationTarget>(NotificationTarget.Folder("trip")), went)
        val after = vm.state.await { !it.saving && it.unread == 3L }
        assertFalse(after.open)
        assertTrue(after.items.orEmpty().single { it.id == "n2" }.read)
        assertEquals(listOf("""{"ids":["n2"]}"""), server.marked)
        assertFalse(vm.select("nope") { went += it })
    }

    @Test
    fun aFileSharedAloneIsFetchedBeforeItOpens() = runReal {
        val vm = NotificationsViewModel(NotificationsServer().api())
        vm.open()
        vm.state.await { it.items != null && !it.loading }
        vm.more()
        vm.state.await { it.cursor == null && !it.loadingMore }
        val went = CompletableDeferred<NotificationTarget>()
        vm.select("n3") { went.complete(it) }
        val target = withTimeout(5_000) { went.await() }
        assertEquals("tax.pdf", (target as NotificationTarget.FileNode).node.name)
        assertFalse(vm.state.value.open)
    }

    @Test
    fun markAllReadEmptiesTheCount() = runReal {
        val server = NotificationsServer()
        val vm = NotificationsViewModel(server.api())
        vm.open()
        vm.state.await { it.items != null && !it.loading && it.unread == 4L }
        vm.markAllRead()
        assertEquals(0L, vm.state.value.unread, "shown read at once")
        val done = vm.state.await { !it.saving }
        assertEquals(0L, done.unread)
        assertTrue(done.items.orEmpty().all { it.read })
        assertFalse("ids" in server.marked.single())
    }

    @Test
    fun aFailureSaysWhy() = runReal {
        val vm = NotificationsViewModel(testApi { respondError(HttpStatusCode.BadGateway) })
        vm.open()
        assertEquals(app.squirrel.ui.photos.UNREACHABLE, vm.state.await { !it.loading && it.error != null }.error)
    }

    @Test
    fun targetsFollowTheSharedSection() {
        fun subject(kind: SubjectKind, mime: String? = null, folder: String? = null) = NotificationSubjectDto("x", kind, "X", folderId = folder, mime = mime)
        assertEquals(NotificationTarget.Folder("x"), targetOf(subject(SubjectKind.folder)))
        assertEquals(NotificationTarget.Album("x", "X"), targetOf(subject(SubjectKind.album)))
        assertEquals(NotificationTarget.Folder("f"), targetOf(subject(SubjectKind.file, "image/jpeg", "f")))
        assertEquals(NotificationTarget.Photo("x", "X"), targetOf(subject(SubjectKind.file, "image/jpeg")))
        assertEquals(NotificationTarget.File("x"), targetOf(subject(SubjectKind.file, "application/pdf")))
    }

    @Test
    fun theBadgeStopsAt99() {
        assertNull(unreadBadge(0))
        assertEquals("7", unreadBadge(7))
        assertEquals("99+", unreadBadge(120))
    }
}
