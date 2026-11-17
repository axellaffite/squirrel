package app.squirrel.ui.photos

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.client.request.HttpRequestData
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
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
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class PhotosViewModelTest {
    private val requests = mutableListOf<HttpRequestData>()
    private var failPhotos = false

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun asset(id: String, w: Int = 4000, h: Int = 3000) =
        """{"id":"$id","kind":"photo","name":"$id.jpg","taken_at":"2024-07-14T18:30:05","thumbnail":{"state":"ready","version":1},""" +
            """"blurhash":null,"width":$w,"height":$h,"mime":"image/jpeg","taken_offset":"+02:00"}"""

    private suspend fun api(): SquirrelApi {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
        val engine = MockEngine { request ->
            requests += request
            val json = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())
            when (request.url.encodedPath) {
                "/api/photos/months" -> respond("""[{"month":"2024-07","count":3},{"month":"2024-06","count":150}]""", headers = json)
                "/api/photos" -> {
                    if (failPhotos) return@MockEngine respondError(HttpStatusCode.BadGateway)
                    val month = request.url.parameters["month"]
                    val cursor = request.url.parameters["cursor"]
                    val body = when {
                        month == "2024-07" -> """{"items":[${asset("a1")},${asset("a2")},${asset("a3")}],"next_cursor":null}"""
                        cursor == null -> """{"items":[${(1..100).joinToString(",") { asset("j$it") }}],"next_cursor":"c2"}"""
                        else -> """{"items":[${(101..150).joinToString(",") { asset("j$it") }}],"next_cursor":null}"""
                    }
                    respond(body, headers = json)
                }
                else -> respondError(HttpStatusCode.NotFound)
            }
        }
        return SquirrelApi("https://home.example", engine, session)
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private suspend fun PhotosViewModel.settled(): PhotosState = withTimeout(5_000) { state.first { !it.busy } }

    @Test
    fun monthsLoadFirstThenEachMonthPageByPage() = run {
        val vm = PhotosViewModel(api(), pageSize = 100)
        val start = vm.settled()
        assertEquals(listOf("2024-07", "2024-06"), start.months.map { it.month })
        assertTrue(start.months.all { it.items.isEmpty() })

        vm.ensure("2024-06")
        assertEquals(100, vm.settled().months[1].items.size)
        vm.ensure("2024-06")
        val june = vm.settled().months[1]
        assertEquals(150, june.items.size)
        assertTrue(june.complete)
        vm.ensure("2024-06") // complete: nothing more to ask
        assertEquals(2, requests.count { it.url.parameters["month"] == "2024-06" })
    }

    @Test
    fun aFailedMonthCanBeRetried() = run {
        failPhotos = true
        val vm = PhotosViewModel(api(), pageSize = 100)
        vm.settled()
        vm.ensure("2024-07")
        val failed = vm.settled()
        assertTrue(failed.months[0].failed)
        assertEquals("Your Squirrel could not be reached. Check your connection and try again.", failed.error)

        failPhotos = false
        vm.retry()
        vm.ensure("2024-07")
        val ok = vm.settled()
        assertEquals(3, ok.months[0].items.size)
        assertNull(ok.error)
    }

    @Test
    fun blocksArePlaceholdersUntilAMonthLoads() = run {
        val vm = PhotosViewModel(api(), pageSize = 100)
        val blocks = timelineBlocks(vm.settled().months, width = 375)
        assertIs<TimelineBlock.Header>(blocks[0])
        assertEquals("July 2024", (blocks[0] as TimelineBlock.Header).label)
        assertTrue(blocks.drop(1).takeWhile { it !is TimelineBlock.Header }.all { it is TimelineBlock.Placeholder })
        assertEquals(1 + 1 + 1 + 50, blocks.size) // July: header + 1 row; June: header + 50 rows of 3

        vm.ensure("2024-07")
        val loaded = timelineBlocks(vm.settled().months, width = 375)
        val row = assertIs<TimelineBlock.Row>(loaded[1])
        // Three 4:3 photos fill a 375 dp row at 92 dp (under the 112 dp target), as on the web.
        assertEquals(listOf("a1", "a2", "a3"), row.tiles.map { it.item.id })
        assertEquals(92, row.height)
    }

    @Test
    fun theViewerPagesThroughEveryLoadedPhotoInOrder() = run {
        val vm = PhotosViewModel(api(), pageSize = 100)
        vm.settled()
        vm.ensure("2024-07")
        vm.ensure("2024-06")
        val state = vm.settled()
        val all = state.loaded()
        assertEquals(103, all.size)
        assertEquals(3, state.indexOf("j1"))
        assertEquals("a1", all[state.indexOf("a1")].id)
        assertEquals(-1, state.indexOf("missing"))
    }
}
