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
import kotlin.test.assertTrue
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class AlbumsRefreshTest {
    private var albums = listOf("Holidays")
    private var photos = (1..6).map { "p$it" }
    private var listCalls = 0
    private var clock = Duration.ZERO

    /** When set, the album list waits for it. */
    private var listGate: CompletableDeferred<Unit>? = null
    private val askedList = CompletableDeferred<Unit>()

    /** When set, the album's second page waits for it. */
    private var pageGate: CompletableDeferred<Unit>? = null
    private val askedPage = CompletableDeferred<Unit>()

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun asset(id: String) =
        """{"id":"$id","kind":"photo","name":"$id.jpg","taken_at":"2024-07-14T18:30:05","thumbnail":{"state":"ready","version":1},""" +
            """"blurhash":null,"width":4000,"height":3000,"mime":"image/jpeg","taken_offset":"+02:00"}"""

    private fun album(name: String) =
        """{"id":"$name","name":"$name","item_count":1,"cover":null,"updated_at":"2026-11-16T12:00:00Z"}"""

    private suspend fun api(): SquirrelApi {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
        val json = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())
        val engine = MockEngine { request ->
            when (request.url.encodedPath) {
                "/api/albums" -> {
                    listCalls++
                    listGate?.let { askedList.complete(Unit); it.await() }
                    respond(albums.joinToString(",", "[", "]") { album(it) }, headers = json)
                }
                "/api/albums/al/items" -> {
                    val from = request.url.parameters["cursor"]?.toInt() ?: 0
                    if (from > 0) pageGate?.let { askedPage.complete(Unit); it.await() }
                    val page = photos.drop(from).take(PAGE)
                    val next = if (from + PAGE < photos.size) "\"${from + PAGE}\"" else "null"
                    respond("""{"items":[${page.joinToString(",") { asset(it) }}],"next_cursor":$next}""", headers = json)
                }
                else -> respondError(HttpStatusCode.NotFound)
            }
        }
        return SquirrelApi("https://home.example", engine, session)
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private suspend fun AlbumsViewModel.settled() = withTimeout(5_000) { state.first { !it.loading } }
    private suspend fun AlbumViewModel.settled() = withTimeout(5_000) { state.first { !it.loading } }

    @Test
    fun theAlbumListStaysWhileItRefreshes() = run {
        val vm = AlbumsViewModel(api(), ResumeThrottle(now = { clock }))
        assertEquals(listOf("Holidays"), vm.settled().albums?.map { it.name })

        albums = listOf("Holidays", "Birthday")
        listGate = CompletableDeferred()
        vm.refresh()
        withTimeout(5_000) { askedList.await() }
        assertTrue(vm.state.value.refreshing)
        assertEquals(listOf("Holidays"), vm.state.value.albums?.map { it.name })
        listGate!!.complete(Unit)
        assertEquals(listOf("Holidays", "Birthday"), vm.settled().albums?.map { it.name })
        assertEquals(false, vm.state.value.refreshing)
    }

    @Test
    fun theAlbumListReloadsOnResumeAtMostEveryThirtySeconds() = run {
        val vm = AlbumsViewModel(api(), ResumeThrottle(now = { clock }))
        vm.settled()
        clock = 20.seconds
        vm.resumed()
        assertEquals(1, listCalls)
        clock = 50.seconds
        vm.resumed()
        vm.settled()
        assertEquals(2, listCalls)
    }

    @Test
    fun anAlbumRefreshDuringALoadMoreNeitherDuplicatesNorDrops() = run {
        val vm = AlbumViewModel(api(), "al", pageSize = PAGE, throttle = ResumeThrottle(now = { clock }))
        assertEquals(listOf("p1", "p2", "p3", "p4"), vm.settled().items.map { it.id })
        pageGate = CompletableDeferred()
        vm.more()
        withTimeout(5_000) { askedPage.await() }

        photos = listOf("p0") + photos
        vm.refresh()
        assertTrue(vm.state.value.refreshing)
        val refreshed = vm.settled()
        assertEquals(listOf("p0", "p1", "p2", "p3"), refreshed.items.map { it.id })
        pageGate!!.complete(Unit)
        delay(200)
        assertEquals(listOf("p0", "p1", "p2", "p3"), vm.state.value.items.map { it.id })

        pageGate = null
        vm.more()
        val all = vm.settled().items.map { it.id }
        assertEquals((0..6).map { "p$it" }, all)
        assertEquals(true, vm.state.value.complete)
    }

    private companion object {
        const val PAGE = 4
    }
}
