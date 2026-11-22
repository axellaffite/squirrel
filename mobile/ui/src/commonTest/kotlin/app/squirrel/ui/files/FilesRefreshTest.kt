package app.squirrel.ui.files

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import app.squirrel.ui.ResumeThrottle
import app.squirrel.ui.photos.UNREACHABLE
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class FilesRefreshTest {
    /** What the root's first page lists; the test changes it to stand for changes on the server. */
    private var firstPage = listOf("a", "b")
    private var secondPage = listOf("c")
    private var failFirstPage = false

    /** When set, the next page (with a cursor) waits for it. */
    private var pageGate: CompletableDeferred<Unit>? = null
    private val askedPage = CompletableDeferred<Unit>()

    /** When set, the next first page waits for it. */
    private var firstGate: CompletableDeferred<Unit>? = null
    private val askedFirst = CompletableDeferred<Unit>()

    private var listings = 0
    private var clock = Duration.ZERO

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun node(id: String) =
        """{"id":"$id","parent_id":"root","name":"$id.pdf","kind":"file","size":1000,"mime":"application/pdf","is_root":false,""" +
            """"created_at":"2026-11-16T12:00:00Z","modified_at":"2026-11-16T12:00:00Z","thumbnail":null}"""

    private suspend fun vm(): FilesViewModel {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
        val json = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())
        val engine = MockEngine { request ->
            if (request.url.encodedPath == "/api/nodes/docs") return@MockEngine respond("""{"node":${node("docs")},"path":[]}""", headers = json)
            if (request.url.encodedPath != "/api/nodes") return@MockEngine respondError(HttpStatusCode.NotFound)
            listings++
            if (request.url.parameters["cursor"] == null) {
                firstGate?.let { askedFirst.complete(Unit); it.await() }
                if (failFirstPage) return@MockEngine respondError(HttpStatusCode.BadGateway)
                respond("""{"items":[${firstPage.joinToString(",") { node(it) }}],"next_cursor":"c2"}""", headers = json)
            } else {
                pageGate?.let { askedPage.complete(Unit); it.await() }
                respond("""{"items":[${secondPage.joinToString(",") { node(it) }}],"next_cursor":null}""", headers = json)
            }
        }
        return FilesViewModel(SquirrelApi("https://home.example", engine, session), null, throttle = ResumeThrottle(now = { clock }))
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private suspend fun FilesViewModel.settled() = withTimeout(5_000) { state.first { !it.loading } }

    private val FilesState.ids get() = items.map { it.id }

    @Test
    fun aRefreshKeepsTheRowsUntilTheNewFirstPageReplacesThem() = run {
        val vm = vm()
        vm.settled()
        vm.more()
        assertEquals(listOf("a", "b", "c"), vm.settled().ids)

        firstPage = listOf("new", "a", "b")
        firstGate = CompletableDeferred()
        vm.refresh()
        withTimeout(5_000) { askedFirst.await() }
        val during = vm.state.value
        assertTrue(during.refreshing)
        assertEquals(listOf("a", "b", "c"), during.ids) // nothing blanks meanwhile
        assertEquals(false, during.firstLoad)

        firstGate!!.complete(Unit)
        val after = vm.settled()
        assertEquals(listOf("new", "a", "b"), after.ids)
        assertEquals(false, after.refreshing)
        assertEquals("c2", after.nextCursor) // paging starts over from the new first page
    }

    @Test
    fun aRefreshDuringALoadMoreNeitherDuplicatesNorDrops() = run {
        val vm = vm()
        vm.settled()
        pageGate = CompletableDeferred()
        vm.more() // page two, stuck
        withTimeout(5_000) { askedPage.await() }

        firstPage = listOf("new", "a", "b")
        vm.refresh() // cancels page two
        val refreshed = vm.settled()
        assertEquals(listOf("new", "a", "b"), refreshed.ids)
        pageGate!!.complete(Unit)
        delay(200)
        assertEquals(listOf("new", "a", "b"), vm.state.value.ids) // the old page two never lands

        pageGate = null
        vm.more() // paging carries on from the refreshed page, once
        assertEquals(listOf("new", "a", "b", "c"), vm.settled().ids)
        vm.more()
        assertEquals(listOf("new", "a", "b", "c"), vm.settled().ids)
    }

    @Test
    fun aLoadMoreDuringARefreshWaitsForIt() = run {
        val vm = vm()
        vm.settled()
        firstGate = CompletableDeferred()
        vm.refresh()
        withTimeout(5_000) { askedFirst.await() }
        val before = listings
        vm.more() // the refresh owns the folder: no page from the old cursor
        assertEquals(before, listings)
        firstGate!!.complete(Unit)
        assertEquals(listOf("a", "b"), vm.settled().ids)
    }

    @Test
    fun aFailedRefreshKeepsTheRowsAndTryAgainRefreshes() = run {
        val vm = vm()
        vm.settled()
        failFirstPage = true
        vm.refresh()
        val failed = vm.settled()
        assertEquals(UNREACHABLE, failed.error)
        assertEquals(listOf("a", "b"), failed.ids)

        failFirstPage = false
        firstPage = listOf("a", "b", "z")
        vm.retry()
        val ok = vm.settled()
        assertNull(ok.error)
        assertEquals(listOf("a", "b", "z"), ok.ids)
    }

    @Test
    fun aRefreshStaysInTheOpenFolder() = run {
        val vm = vm()
        vm.settled()
        vm.enter("docs") // the mock lists the same rows in any folder
        assertEquals(listOf("docs"), vm.settled().path.map { it.id })
        firstPage = listOf("inside")
        vm.refresh()
        val after = vm.settled()
        assertEquals("docs", after.folderId)
        assertEquals(listOf("docs"), after.path.map { it.id })
        assertEquals(listOf("inside"), after.ids)
    }

    @Test
    fun comingBackReloadsAtMostEveryThirtySeconds() = run {
        val vm = vm()
        vm.settled()
        val start = listings
        clock = 10.seconds
        vm.resumed()
        assertEquals(start, listings) // just loaded: nothing

        clock = 31.seconds
        firstPage = listOf("fresh")
        vm.resumed()
        assertEquals(listOf("fresh"), vm.settled().ids)
        assertEquals(start + 1, listings)

        clock = 40.seconds
        vm.resumed()
        assertEquals(start + 1, listings)
    }
}
