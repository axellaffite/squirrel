package app.squirrel.ui.photos

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import app.squirrel.ui.ResumeThrottle
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
import kotlin.test.assertSame
import kotlin.test.assertTrue
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class PhotosRefreshTest {
    /** Month to its photo ids, newest month first; changed by the tests as the server would. */
    private var library = linkedMapOf("2024-07" to listOf("a1", "a2"), "2024-06" to (1..6).map { "j$it" })
    private var failMonths = false
    private val monthRequests = mutableListOf<String>()
    private var monthCalls = 0

    /** When set, the month list waits for it. */
    private var monthsGate: CompletableDeferred<Unit>? = null
    private val askedMonths = CompletableDeferred<Unit>()
    private var clock = Duration.ZERO

    /** When set, July's first page waits for it. */
    private var julyGate: CompletableDeferred<Unit>? = null
    private val askedJuly = CompletableDeferred<Unit>()

    /** When set, June's second page waits for it. */
    private var juneGate: CompletableDeferred<Unit>? = null
    private val askedJune = CompletableDeferred<Unit>()

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun asset(id: String) =
        """{"id":"$id","kind":"photo","name":"$id.jpg","taken_at":"2024-07-14T18:30:05","thumbnail":{"state":"ready","version":1},""" +
            """"blurhash":null,"width":4000,"height":3000,"mime":"image/jpeg","taken_offset":"+02:00"}"""

    private suspend fun vm(pageSize: Int = 4): PhotosViewModel {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
        val json = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())
        val engine = MockEngine { request ->
            when (request.url.encodedPath) {
                "/api/photos/months" -> {
                    monthCalls++
                    monthsGate?.let { askedMonths.complete(Unit); it.await() }
                    if (failMonths) return@MockEngine respondError(HttpStatusCode.BadGateway)
                    respond(library.entries.joinToString(",", "[", "]") { """{"month":"${it.key}","count":${it.value.size}}""" }, headers = json)
                }
                "/api/photos" -> {
                    val month = request.url.parameters["month"]!!
                    val cursor = request.url.parameters["cursor"]
                    monthRequests += month
                    if (month == "2024-07" && cursor == null) julyGate?.let { askedJuly.complete(Unit); it.await() }
                    val stuck = juneGate
                    if (month == "2024-06" && cursor != null && stuck != null) {
                        askedJune.complete(Unit)
                        stuck.await()
                        // What that old page would add: ids the fresh pages never have.
                        return@MockEngine respond("""{"items":[${asset("old5")},${asset("old6")}],"next_cursor":null}""", headers = json)
                    }
                    val ids = library[month].orEmpty()
                    val from = cursor?.toInt() ?: 0
                    val page = ids.drop(from).take(pageSize)
                    val next = if (from + pageSize < ids.size) "\"${from + pageSize}\"" else "null"
                    respond("""{"items":[${page.joinToString(",") { asset(it) }}],"next_cursor":$next}""", headers = json)
                }
                else -> respondError(HttpStatusCode.NotFound)
            }
        }
        return PhotosViewModel(SquirrelApi("https://home.example", engine, session), pageSize, ResumeThrottle(now = { clock }))
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private suspend fun PhotosViewModel.settled(): PhotosState =
        withTimeout(5_000) { state.first { !it.busy && !it.refreshing } }

    private fun PhotosState.ids(month: String) = months.first { it.month == month }.items.map { it.id }

    @Test
    fun newPhotosInAMonthReplaceItsFirstPageOnlyOnceItArrives() = run {
        val vm = vm()
        vm.settled()
        vm.ensure("2024-07")
        vm.ensure("2024-06")
        val before = vm.settled()
        val june = before.months[1]

        library["2024-07"] = listOf("a0", "a1", "a2") // a photo was backed up meanwhile
        julyGate = CompletableDeferred()
        vm.refresh()
        withTimeout(5_000) { askedJuly.await() }
        val during = vm.state.value
        assertTrue(during.refreshing)
        assertEquals(listOf("a1", "a2"), during.ids("2024-07")) // still showing the old rows
        assertEquals(false, during.loadingMonths) // no "Loading…" over the timeline

        julyGate!!.complete(Unit)
        val after = vm.settled()
        assertEquals(listOf("a0", "a1", "a2"), after.ids("2024-07"))
        assertEquals(3L, after.months[0].count)
        assertSame(june, after.months[1]) // unchanged month: same page, not reloaded
        assertEquals(1, monthRequests.count { it == "2024-06" })
    }

    @Test
    fun aNewMonthAppearsAtTheTop() = run {
        val vm = vm()
        vm.settled()
        library = linkedMapOf("2024-08" to listOf("n1")).apply { putAll(library) }
        vm.refresh()
        val after = vm.settled()
        assertEquals(listOf("2024-08", "2024-07", "2024-06"), after.months.map { it.month })
    }

    @Test
    fun aRefreshDuringAMonthsNextPageNeitherDuplicatesNorDrops() = run {
        val vm = vm()
        vm.settled()
        vm.ensure("2024-06")
        assertEquals(listOf("j1", "j2", "j3", "j4"), vm.settled().ids("2024-06"))
        juneGate = CompletableDeferred()
        vm.ensure("2024-06") // June's second page, stuck
        withTimeout(5_000) { askedJune.await() }

        library["2024-06"] = listOf("j0") + (1..6).map { "j$it" }
        val stuck = juneGate!!
        juneGate = null
        vm.refresh()
        // June's new first page, then (it was on screen, paging) its next one from the new cursor.
        val expected = listOf("j0") + (1..6).map { "j$it" }
        assertEquals(expected, vm.settled().ids("2024-06"))

        stuck.complete(Unit)
        delay(200)
        val final = vm.state.value.months[1]
        assertEquals(expected, final.items.map { it.id }) // the old page never lands
        assertTrue(final.complete)
        assertEquals(false, final.loading)
    }

    @Test
    fun aFailedRefreshKeepsTheTimelineAndTryAgainRefreshes() = run {
        val vm = vm()
        vm.settled()
        vm.ensure("2024-07")
        vm.settled()
        failMonths = true
        vm.refresh()
        val failed = vm.settled()
        assertEquals(UNREACHABLE, failed.error)
        assertEquals(listOf("a1", "a2"), failed.ids("2024-07"))

        failMonths = false
        library["2024-07"] = listOf("a0", "a1", "a2")
        vm.retry()
        val ok = vm.settled()
        assertNull(ok.error)
        assertEquals(listOf("a0", "a1", "a2"), ok.ids("2024-07"))
    }

    @Test
    fun anEmptyLibraryCanBeRefreshedWithoutLoadingAgain() = run {
        library = linkedMapOf()
        val vm = vm()
        assertTrue(vm.settled().empty)
        library = linkedMapOf("2024-07" to listOf("a1"))
        vm.refresh()
        assertEquals(false, vm.state.value.loadingMonths)
        assertEquals(listOf("2024-07"), vm.settled().months.map { it.month })
    }

    @Test
    fun comingBackReloadsAtMostEveryThirtySeconds() = run {
        val vm = vm()
        vm.settled()
        assertEquals(1, monthCalls)
        clock = 29.seconds
        vm.resumed()
        assertEquals(1, monthCalls)
        clock = 30.seconds
        vm.resumed()
        vm.settled()
        assertEquals(2, monthCalls)
        vm.resumed()
        assertEquals(2, monthCalls)
    }

    @Test
    fun tryAgainDuringARefreshDoesNotLoadTheMonthsTwice() = run {
        library = linkedMapOf()
        val vm = vm()
        vm.settled()
        monthsGate = CompletableDeferred()
        vm.refresh()
        withTimeout(5_000) { askedMonths.await() }
        vm.retry() // the empty timeline's "Try again" while the pull is still loading
        assertEquals(false, vm.state.value.loadingMonths)
        monthsGate!!.complete(Unit)
        vm.settled()
        assertEquals(2, monthCalls) // the first load and the refresh, nothing more
    }
}
