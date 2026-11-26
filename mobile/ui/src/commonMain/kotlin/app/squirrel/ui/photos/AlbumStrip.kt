package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.AlbumDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Tokens
import app.squirrel.ui.OnForeground
import app.squirrel.ui.explore.SectionHead
import app.squirrel.ui.explore.photoCount

/** How many albums the strip shows; "See all" has the rest. */
const val STRIP_ALBUMS = 10

/** The albums the strip shows: the [STRIP_ALBUMS] most recently updated, newest first. */
fun stripAlbums(albums: List<AlbumDto>): List<AlbumDto> = albums.sortedByDescending { it.updatedAt }.take(STRIP_ALBUMS)

/**
 * MASTER "Album strip": the most recent albums above the timeline ([stripAlbums]), one row of album cards `--album-min` wide
 * that scrolls sideways, under an "Albums" heading with "See all" ([onSeeAll]). Nothing while
 * there is no album, or the list has not loaded: the timeline comes first.
 */
@Composable
fun AlbumStrip(api: SquirrelApi, vm: AlbumsViewModel, onOpen: (AlbumDto) -> Unit, onSeeAll: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    OnForeground(vm::resumed)
    val albums = remember(state.albums) { stripAlbums(state.albums.orEmpty()) }
    if (albums.isEmpty()) return
    Column {
        SectionHead("Albums", onSeeAll = onSeeAll)
        LazyRow(
            horizontalArrangement = Arrangement.spacedBy(Tokens.space3),
            // Room for the cards' shadow, which the row would clip.
            contentPadding = PaddingValues(vertical = Tokens.space2),
        ) {
            items(albums, key = { it.id }) { album ->
                CoverCard(api, album.cover, album.name, photoCount(album.itemCount), onClick = { onOpen(album) }, modifier = Modifier.width(Tokens.albumMin))
            }
        }
    }
}
