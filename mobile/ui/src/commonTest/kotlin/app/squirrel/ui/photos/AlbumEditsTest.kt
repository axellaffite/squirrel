package app.squirrel.ui.photos

import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.MemberDto
import app.squirrel.api.models.SharePermission
import app.squirrel.api.models.SharedItemDto
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import app.squirrel.ui.testing.text
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.time.Instant

class AlbumEditsTest {
    private val at = Instant.parse("2026-11-20T17:05:00Z")

    private fun album(id: String, name: String) = """{"id":"$id","name":"$name","item_count":1,"updated_at":"2026-11-20T17:05:00Z","access":"owner"}"""

    private fun dto(id: String, name: String) = AlbumDto(id = id, itemCount = 0, name = name, updatedAt = at)

    @Test
    fun aNewAlbumIsCreatedOnceEvenWhenALaterBatchIsRetried() = runReal {
        var creates = 0
        var addCalls = 0
        val api = testApi { request ->
            when (request.url.encodedPath) {
                "/api/albums" -> { creates += 1; json(album("new", "Trip").replace("\"item_count\":1", "\"item_count\":500"), HttpStatusCode.Created) }
                "/api/albums/new/items" -> {
                    addCalls += 1
                    if (addCalls == 1) respondError(HttpStatusCode.BadGateway) else json("""{"changed":2}""")
                }
                else -> respondError(HttpStatusCode.NotFound)
            }
        }
        val adder = AlbumAdder(api, (1..502).map { "p$it" })
        assertEquals(true, adder.intoNew("Trip") is Outcome.Failed)
        assertEquals(Outcome.Ok("Added 502 photos to Trip."), adder.intoNew("Trip"))
        assertEquals(1, creates)
        assertEquals(2, addCalls)
    }

    @Test
    fun removalsAddUpAndSayWhenNothingWasYours() = runReal {
        val bodies = mutableListOf<String>()
        val api = testApi { request ->
            bodies += request.text()
            if (request.url.encodedPath == "/api/albums/al/items/remove") json("""{"changed":1}""") else respondError(HttpStatusCode.NotFound)
        }
        assertEquals(Outcome.Ok(2L), removeFromAlbum(api, "al", (1..501).map { "p$it" }))
        assertEquals(2, bodies.size)
        assertEquals("Removed 2 photos from the album.", removedMessage(2))
        assertEquals("Nothing was removed: you can only remove the photos you added.", removedMessage(0))
    }

    @Test
    fun picksAreOwnAndWritableAlbumsByName() {
        val owner = MemberDto(displayName = "Anna", username = "anna")
        val shared = listOf(
            SharedItemDto(owner = owner, permission = SharePermission.write, shareId = "s1", sharedAt = at, album = dto("w", "beach")),
            SharedItemDto(owner = owner, permission = SharePermission.read, shareId = "s2", sharedAt = at, album = dto("r", "Read only")),
        )
        assertEquals(listOf("beach", "Zoo"), writableAlbums(listOf(dto("z", "Zoo")), shared).map { it.name })
        // A dynamic album takes no photos by hand: never offered.
        val dynamic = dto("d", "Rules").copy(dynamic = true)
        val sharedDynamic = SharedItemDto(owner = owner, permission = SharePermission.write, shareId = "s3", sharedAt = at, album = dto("sd", "Theirs").copy(dynamic = true))
        assertEquals(listOf("Zoo"), writableAlbums(listOf(dto("z", "Zoo"), dynamic), listOf(sharedDynamic)).map { it.name })
        assertEquals("This photo is already in Zoo.", addedMessage(0, 1, "Zoo"))
        assertEquals("These photos are already in Zoo.", addedMessage(0, 3, "Zoo"))
        assertEquals("Added 1 photo to Zoo.", addedMessage(1, 3, "Zoo"))
        assertEquals(setOf("a"), emptySet<String>().toggled("a"))
        assertEquals(emptySet(), setOf("a").toggled("a"))
    }
}
