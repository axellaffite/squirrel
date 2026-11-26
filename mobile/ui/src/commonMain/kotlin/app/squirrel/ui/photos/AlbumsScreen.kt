package app.squirrel.ui.photos

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.ThumbnailStatus
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.sync.formatCount
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.cardShadow
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.PullToRefresh
import app.squirrel.ui.OnForeground
import app.squirrel.ui.PullableColumn
import app.squirrel.theme.tabular
import app.squirrel.ui.images.Thumbnail
import app.squirrel.ui.explore.photoCount
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.RowGroup
import app.squirrel.theme.components.Sheet

/** MASTER.md album cards: two columns of white cards, a 4:3 cover, the name and the count. */
@Composable
fun AlbumsGrid(api: SquirrelApi, vm: AlbumsViewModel, onOpen: (AlbumDto) -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    OnForeground(vm::resumed)
    val albums = state.albums
    when {
        albums == null && state.error != null -> return PullableColumn(state.loading, vm::refresh) { ErrorRetry(state.error!!, vm::load) }
        albums == null -> return Loading()
        albums.isEmpty() -> return PullableColumn(state.refreshing, vm::refresh) {
            state.error?.let { ErrorRetry(it, vm::load) }
            EmptyState(SquirrelIcon.Library, "No albums yet", "Make one with the + button, or select photos in the timeline and add them.")
        }
    }
    PullToRefresh(state.refreshing, vm::refresh) {
        LazyVerticalGrid(
            columns = GridCells.Fixed(2),
            modifier = Modifier.fillMaxSize(),
            horizontalArrangement = Arrangement.spacedBy(Tokens.space3),
            verticalArrangement = Arrangement.spacedBy(Tokens.space3),
            contentPadding = PaddingValues(top = Tokens.space2, bottom = RailClearance),
        ) {
            // A failed refresh keeps the albums shown, with "Try again" above them.
            state.error?.let { error -> item(key = "error", span = { GridItemSpan(maxLineSpan) }) { ErrorRetry(error, vm::load) } }
            items(albums.orEmpty(), key = { it.id }) { album -> AlbumCard(api, album) { onOpen(album) } }
        }
    }
}

@Composable
private fun AlbumCard(api: SquirrelApi, album: AlbumDto, onClick: () -> Unit) {
    CoverCard(api, album.cover, album.name, albumMeta(album), onClick = onClick, icon = albumIcon(album))
}

/** "12 photos", and "· Dynamic" for an album whose photos come from rules. */
fun albumMeta(album: AlbumDto): String = if (album.dynamic) "${photoCount(album.itemCount)} · Dynamic" else photoCount(album.itemCount)

/** An empty album's cover icon: sparkles mark a dynamic one. */
fun albumIcon(album: AlbumDto): SquirrelIcon = if (album.dynamic) SquirrelIcon.Sparkles else SquirrelIcon.Library

/**
 * The + button's choice: an album one fills by hand, or a dynamic one whose photos come from
 * rules (people, tags, place…).
 */
@Composable
fun NewAlbumSheet(onManual: () -> Unit, onDynamic: () -> Unit, onClose: () -> Unit) {
    Sheet("New album", onDismiss = onClose) {
        RowGroup {
            ListRow("Album", icon = SquirrelIcon.Library, meta = "Choose its photos yourself", onClick = onManual)
            ListRow("Dynamic album", icon = SquirrelIcon.Sparkles, meta = "Filled by rules: people, tags, places…", onClick = onDynamic)
        }
    }
}

/** One album's photos in justified rows, or as a timeline (month headings) when [dated]. */
@Composable
fun AlbumPhotos(api: SquirrelApi, vm: AlbumViewModel, actions: TileActions, emptyLine: String, dated: Boolean = false) {
    val state by vm.state.collectAsStateWithLifecycle()
    OnForeground(vm::resumed)
    if (!state.started && state.loading) return Loading()
    if (state.items.isEmpty() && (state.complete || state.error != null)) {
        return PullableColumn(state.refreshing, vm::refresh) {
            state.error?.let { ErrorRetry(it, vm::retry) }
            if (state.complete) EmptyState(SquirrelIcon.Images, "This album is empty", emptyLine)
        }
    }
    val footer: LazyListScope.() -> Unit = { state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::retry) } } }
    PullToRefresh(state.refreshing, vm::refresh) {
        if (dated) DatedPhotos(api, state.items, actions, onNearEnd = vm::more, footer = footer)
        else JustifiedPhotos(api, state.items, actions, onNearEnd = vm::more, footer = footer)
    }
}
