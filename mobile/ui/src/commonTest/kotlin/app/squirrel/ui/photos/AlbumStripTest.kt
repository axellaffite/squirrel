package app.squirrel.ui.photos

import app.squirrel.api.models.AlbumDto
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.time.Instant

class AlbumStripTest {
    private fun album(id: String, updatedAt: String) = AlbumDto(id = id, itemCount = 1, name = id, updatedAt = Instant.parse(updatedAt))

    @Test
    fun theStripShowsTheMostRecentlyUpdatedAlbumsFirst() {
        val albums = listOf(album("old", "2024-01-01T00:00:00Z"), album("new", "2026-11-20T00:00:00Z"), album("mid", "2025-06-01T00:00:00Z"))
        assertEquals(listOf("new", "mid", "old"), stripAlbums(albums).map { it.id })
    }

    @Test
    fun theStripKeepsTenAlbumsAtMost() {
        val albums = (1..15).map { album("a$it", "2026-11-${it.toString().padStart(2, '0')}T00:00:00Z") }
        assertEquals((15 downTo 6).map { "a$it" }, stripAlbums(albums).map { it.id })
    }
}
