package app.squirrel.ui.photos

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.AssetSummary
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
            EmptyState(SquirrelIcon.Library, "No albums yet", "Albums you make on the web appear here.")
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
    val scheme = SquirrelTheme.scheme
    val cover = album.cover
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .cardShadow(Shapes.card)
            .clip(Shapes.card)
            .background(scheme.card)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(Tokens.space2),
        verticalArrangement = Arrangement.spacedBy(Tokens.space1),
    ) {
        Thumbnail(
            url = cover?.takeIf { it.thumbnail.state == ThumbnailStatus.ready }?.let { api.thumbnailUrl(it.id, it.thumbnail.version) },
            blurhash = cover?.blurhash,
            contentDescription = null,
            modifier = Modifier.fillMaxWidth().aspectRatio(Tokens.albumRatio).clip(Shapes.tile),
        )
        BasicText(album.name, style = SquirrelTheme.type.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
        BasicText(
            "${formatCount(album.itemCount)} ${if (album.itemCount == 1L) "photo" else "photos"}",
            style = SquirrelTheme.type.caption.tabular(),
        )
    }
}

/** One album's photos in justified rows. */
@Composable
fun AlbumPhotos(api: SquirrelApi, vm: AlbumViewModel, onOpen: (AssetSummary) -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    OnForeground(vm::resumed)
    if (!state.started && state.loading) return Loading()
    if (state.items.isEmpty() && (state.complete || state.error != null)) {
        return PullableColumn(state.refreshing, vm::refresh) {
            state.error?.let { ErrorRetry(it, vm::retry) }
            if (state.complete) EmptyState(SquirrelIcon.Images, "This album is empty", "Add photos to it on the web.")
        }
    }
    PullToRefresh(state.refreshing, vm::refresh) { AlbumRows(api, vm, state, onOpen) }
}

@Composable
private fun AlbumRows(api: SquirrelApi, vm: AlbumViewModel, state: AlbumState, onOpen: (AssetSummary) -> Unit) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val width = maxWidth.value.toInt()
        val rowHeight = Tokens.photoRow.value.toInt()
        val gap = Tokens.space1.value.toInt()
        val rows = remember(state.items, width) {
            val layout = justify(state.items.map { a -> (a.width ?: 1).toDouble() / (a.height ?: 1).coerceAtLeast(1) }, width, rowHeight, gap)
            layout.rows.map { row -> row.height to row.tiles.map { TileSlot(state.items[it.index], it.left, it.width) } }
        }
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(Tokens.space1),
            contentPadding = PaddingValues(bottom = RailClearance),
        ) {
            items(rows.size, key = { "row:$it" }) { n ->
                val (height, tiles) = rows[n]
                if (n == rows.lastIndex) LaunchedEffect(state.items.size) { vm.more() }
                PhotoRow(api, height, tiles, onOpen)
            }
            state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::retry) } }
        }
    }
}

