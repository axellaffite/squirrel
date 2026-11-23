package app.squirrel.ui.search

import app.squirrel.api.models.PhotoKindFilter
import app.squirrel.api.models.SearchOrder
import app.squirrel.ui.explore.PhotoFilter
import app.squirrel.ui.testing.apiError
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.summary
import app.squirrel.ui.testing.testApi
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode
import io.ktor.http.Parameters
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class PhotoSearchViewModelTest {
    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private val asked = mutableListOf<Parameters>()
    private var warmedUp = 0

    private suspend fun vm(searches: suspend io.ktor.client.engine.mock.MockRequestHandleScope.(Parameters) -> io.ktor.client.request.HttpResponseData) =
        PhotoSearchViewModel(
            testApi { request ->
                when (request.url.encodedPath) {
                    "/api/search/warmup" -> { warmedUp += 1; json("", HttpStatusCode.NoContent) }
                    "/api/people" -> json("""[{"id":"anna","name":"Anna","hidden":false,"face_count":3}]""")
                    "/api/search/photos" -> { asked += request.url.parameters; searches(request.url.parameters) }
                    else -> respondError(HttpStatusCode.NotFound)
                }
            },
            debounceMs = 1,
            busyRetryMs = 1,
        )

    @Test
    fun typingSearchesTheTimelineWithTheFilter() = runReal {
        val vm = vm { json("""{"items":[${summary("p1")},${summary("p2")}],"has_more":false,"next_cursor":null}""") }
        vm.setFilter(PhotoFilter(tag = "beach", kind = PhotoKindFilter.screenshot))
        vm.type("  dog   on the beach ")
        val done = vm.state.await { it.settled }
        assertEquals(listOf("p1", "p2"), done.items.map { it.id })
        assertEquals("dog on the beach", done.query)
        val params = asked.last()
        assertEquals("dog on the beach", params["q"])
        assertEquals("date", params["order"])
        assertEquals("beach", params["tag"])
        assertEquals("screenshot", params["kind"])
        assertEquals(1, warmedUp)
        assertEquals("Anna", vm.state.await { it.people.isNotEmpty() }.people.single().name)
    }

    @Test
    fun dateOrderPagesByCursorAndRelevanceByOffset() = runReal {
        val vm = vm { p ->
            when {
                p["order"] == "date" && p["cursor"] == null -> json("""{"items":[${summary("a")}],"has_more":true,"next_cursor":"c2"}""")
                p["order"] == "date" -> json("""{"items":[${summary("b")}],"has_more":false,"next_cursor":null}""")
                p["offset"] == "0" -> json("""{"items":[${summary("r1")}],"has_more":true}""")
                else -> json("""{"items":[${summary("r2")}],"has_more":false}""")
            }
        }
        vm.type("cat")
        assertTrue(vm.state.await { it.settled }.hasMore)
        vm.more()
        assertEquals(listOf("a", "b"), vm.state.await { it.items.size == 2 }.items.map { it.id })
        assertEquals("c2", asked.last()["cursor"])

        vm.setOrder(SearchOrder.relevance)
        assertEquals(listOf("r1"), vm.state.await { it.items.map { i -> i.id } == listOf("r1") }.items.map { it.id })
        vm.more()
        assertEquals(listOf("r1", "r2"), vm.state.await { it.items.size == 2 && !it.loading }.items.map { it.id })
        assertEquals("1", asked.last()["offset"])
        assertEquals(false, vm.state.value.hasMore)
    }

    @Test
    fun aServerWithoutTheModelSaysSoOnce() = runReal {
        val vm = vm { apiError(HttpStatusCode.ServiceUnavailable, "ml_unavailable") }
        vm.type("cat")
        val state = vm.state.await { !it.loading && it.unavailable }
        assertEquals(SEARCH_UNAVAILABLE, state.error)
        assertEquals(1, asked.size)
    }

    @Test
    fun aBusyModelIsAskedAgain() = runReal {
        var calls = 0
        val vm = vm {
            calls += 1
            if (calls < 3) apiError(HttpStatusCode.ServiceUnavailable, "busy") else json("""{"items":[${summary("p")}],"has_more":false}""")
        }
        vm.type("cat")
        assertEquals(listOf("p"), vm.state.await { it.settled }.items.map { it.id })
        assertEquals(3, calls)
    }

    @Test
    fun aBusyModelGivesUpAfterItsRetries() = runReal {
        val vm = vm { apiError(HttpStatusCode.ServiceUnavailable, "busy") }
        vm.type("cat")
        assertEquals(SEARCH_BUSY, vm.state.await { !it.loading && it.error != null }.error)
        assertEquals(BUSY_RETRIES + 1, asked.size)
    }

    @Test
    fun rateLimitedTwiceAsksToWait() = runReal {
        val vm = vm { apiError(HttpStatusCode.TooManyRequests, "rate_limited", headers = io.ktor.http.headersOf(
            io.ktor.http.HttpHeaders.ContentType to listOf("application/json"),
            io.ktor.http.HttpHeaders.RetryAfter to listOf("0"),
        )) }
        vm.type("cat")
        assertEquals(STILL_TOO_FAST, vm.state.await { !it.loading && it.error == STILL_TOO_FAST }.error)
        assertEquals(2, asked.size)
    }

    @Test
    fun clearingTheFieldClearsTheResultsAndTooLongIsRefused() = runReal {
        val vm = vm { json("""{"items":[${summary("p")}],"has_more":false}""") }
        vm.type("cat")
        vm.state.await { it.settled }
        vm.type("")
        assertTrue(vm.state.await { it.items.isEmpty() }.query.isEmpty())
        vm.type("x".repeat(MAX_PHOTO_QUERY + 1))
        assertEquals(PHOTO_TOO_LONG, vm.state.value.error)
        assertEquals(1, asked.size)
    }

    @Test
    fun anUnreachableServerIsAnErrorWithoutResults() = runReal {
        val vm = vm { throw io.ktor.client.plugins.HttpRequestTimeoutException("search", 30_000) }
        vm.type("cat")
        val state = vm.state.await { !it.loading && it.error != null }
        assertEquals(app.squirrel.ui.photos.UNREACHABLE, state.error)
        assertTrue(state.items.isEmpty())
    }
}
