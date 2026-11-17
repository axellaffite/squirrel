package app.squirrel.ui.photos

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
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

@OptIn(ExperimentalCoroutinesApi::class)
class AlbumsViewModelTest {
    private var pages = 0

    @BeforeTest
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @AfterTest
    fun tearDown() = Dispatchers.resetMain()

    private fun asset(id: String) =
        """{"id":"$id","kind":"photo","name":"$id.jpg","taken_at":"2024-07-14T18:30:05","thumbnail":{"state":"ready","version":1},""" +
            """"blurhash":null,"width":4000,"height":3000,"mime":"image/jpeg","taken_offset":"+02:00"}"""

    private suspend fun api(): SquirrelApi {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
        val json = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())
        val engine = MockEngine { request ->
            if (request.url.encodedPath != "/api/albums/al/items") return@MockEngine respondError(HttpStatusCode.NotFound)
            pages++
            val body = if (request.url.parameters["cursor"] == null) {
                """{"items":[${(1..20).joinToString(",") { asset("p$it") }}],"next_cursor":"c2"}"""
            } else {
                """{"items":[${(21..25).joinToString(",") { asset("p$it") }}],"next_cursor":null}"""
            }
            respond(body, headers = json)
        }
        return SquirrelApi("https://home.example", engine, session)
    }

    private fun run(block: suspend () -> Unit) = runTest { withContext(Dispatchers.Default) { block() } }

    private suspend fun AlbumViewModel.settled() = withTimeout(5_000) { state.first { !it.loading } }

    @Test
    fun theViewerNearTheEndOfAnAlbumLoadsItsNextPage() = run {
        val vm = AlbumViewModel(api(), "al", pageSize = 20)
        assertEquals(20, vm.settled().items.size)
        vm.near("p5")
        assertEquals(1, pages)
        vm.near("p15")
        val all = vm.settled()
        assertEquals(25, all.items.size)
        assertEquals(true, all.complete)
    }
}
