package app.squirrel.ui.photos

import app.squirrel.api.models.AssetKindDto
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.ThumbnailState
import app.squirrel.api.models.ThumbnailStatus
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.HandoffResult
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.headersOf
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class OpenOriginalTest {
    private val summary =
        """{"id":"v1","kind":"photo","name":"v1.mov","taken_at":"2024-07-14T18:30:05","thumbnail":{"state":"ready","version":1},""" +
            """"blurhash":null,"width":1920,"height":1080,"mime":"video/quicktime","taken_offset":"+02:00"}"""

    private val item = AssetSummary(
        id = "v1",
        kind = AssetKindDto.photo,
        name = "v1.mov",
        takenAt = "2024-07-14T18:30:05",
        thumbnail = ThumbnailState(ThumbnailStatus.ready, 1),
        mime = "video/quicktime",
    )

    private class FakeHandoff : FileHandoff {
        val opened = mutableListOf<String>()
        override suspend fun open(api: SquirrelApi, id: String, name: String, mime: String?, share: Boolean, onProgress: (Float) -> Unit): HandoffResult {
            opened += id
            return HandoffResult.Opened
        }
        override fun clear() = Unit
    }

    private suspend fun api(size: Long): SquirrelApi {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
        val json = headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString())
        val engine = MockEngine {
            respond("""{"asset":$summary,"size":$size,"albums":[]}""", headers = json)
        }
        return SquirrelApi("https://home.example", engine, session)
    }

    @Test
    fun aLargeOriginalAsksFirst() = runTest {
        val handoff = FakeHandoff()
        val open = OpenOriginal(api(1_400_000_000), handoff, this)
        open.request(item)
        val asking = open.state.first { it.confirm != null }
        assertEquals("Download 1.4 GB?", asking.confirmText)
        assertEquals(emptyList(), handoff.opened)

        open.confirm()
        open.state.first { it.progress == null && it.confirm == null }
        testScheduler.advanceUntilIdle()
        assertEquals(listOf("v1"), handoff.opened)
    }

    @Test
    fun aSmallOriginalOpensAtOnce() = runTest {
        val handoff = FakeHandoff()
        val open = OpenOriginal(api(5_000_000), handoff, this)
        open.request(item) // progress shows at once, until the download ends
        open.state.first { it.progress == null }
        assertNull(open.state.value.confirm)
        assertEquals(listOf("v1"), handoff.opened)
    }
}
