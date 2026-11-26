package app.squirrel.ui.search

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.api.models.SearchOrder
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.SearchField
import app.squirrel.theme.components.SegmentedControl
import app.squirrel.ui.AppGraph
import app.squirrel.ui.explore.FilterBar
import app.squirrel.ui.photos.AddToAlbumSelection
import app.squirrel.ui.photos.rememberPhotoDownloads
import app.squirrel.ui.photos.AlbumsViewModel
import app.squirrel.ui.photos.ErrorRetry
import app.squirrel.ui.photos.JustifiedPhotos
import app.squirrel.ui.photos.PhotosOverlay
import app.squirrel.ui.photos.TileActions
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest
import io.github.axellaffite.lsdo.compose.RegisterLsDo

/** What the search screen looks for. */
enum class SearchKind(val label: String) { Photos("Photos"), Files("Files") }

/** Search: photos by what they show (with the timeline's filters), or files by name and text. */
@Composable
fun SearchScreen(graph: AppGraph, start: SearchKind, onBack: () -> Unit) {
    val api = graph.api ?: return
    var kind by rememberSaveable { mutableStateOf(start) }
    SystemBack(onBack = onBack)
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader("Search", onBack = onBack, backLabel = "Close the search")
        SegmentedControl(SearchKind.entries.map { it.label }, kind.ordinal, onSelect = { kind = SearchKind.entries[it] })
        when (kind) {
            SearchKind.Photos -> PhotoSearch(graph, api, viewModel(key = "search-photos") { PhotoSearchViewModel(api) })
            SearchKind.Files -> DocumentSearch(api, viewModel(key = "search-files") { DocumentSearchViewModel(api, graph.files) })
        }
    }
}

private val ORDERS = listOf(SearchOrder.date to "Timeline", SearchOrder.relevance to "Best match")

@Composable
private fun PhotoSearch(graph: AppGraph, api: SquirrelApi, vm: PhotoSearchViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    val albums = viewModel(key = "albums") { AlbumsViewModel(api) }
    var viewing by remember { mutableStateOf<String?>(null) }
    // Kept across searches: photos found by several searches can go into one album together.
    var selected by remember { mutableStateOf(emptySet<String>()) }
    var notice by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val onAdded: (String) -> Unit = { message ->
        notice = message
        selected = emptySet()
        albums.refresh()
    }
    val downloads = rememberPhotoDownloads(api, graph.saver)
    val lsdo = remember(vm) { PhotoSearchLsDo(vm, api, scope, onOpen = { viewing = it }, onClose = { viewing = null }, onAdded = onAdded, downloads = downloads) }
    LaunchedEffect(viewing, notice) { lsdo.overlay.shown(PhotosOverlay(viewing, notice)) }
    RegisterLsDo("photo_search", lsdo)
    SystemBack(enabled = selected.isNotEmpty()) { selected = emptySet() }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            PhotoSearchResults(api, vm, state, TileActions({ viewing = it.id }, selected) { selected = it }, notice)
        }
        AddToAlbumSelection(
            api,
            selected,
            onClear = { selected = emptySet() },
            onAdded = onAdded,
            modifier = Modifier.align(Alignment.BottomCenter),
            downloads = downloads,
            nameOf = { id -> vm.state.value.items.firstOrNull { it.id == id }?.name },
        )
    }
    viewing?.let { id ->
        ViewerHost(graph, ViewerRequest(state.items, id, onNear = { if (state.items.lastOrNull()?.id == it) vm.more() }), onClose = { viewing = null })
    }
}

/** The search field, filters and order, then what they found. */
@Composable
private fun PhotoSearchResults(api: SquirrelApi, vm: PhotoSearchViewModel, state: PhotoSearchState, actions: TileActions, notice: String?) {
    SearchField(
        value = state.typed,
        onValueChange = vm::type,
        label = "Search photos",
        placeholder = "Search your photos",
        onSearch = vm::now,
        autofocus = state.typed.isEmpty(),
    )
    FilterBar(api, state.filter, state.people, vm::setFilter)
    if (!state.unavailable) {
        SegmentedControl(ORDERS.map { it.second }, ORDERS.indexOfFirst { it.first == state.order }, onSelect = { vm.setOrder(ORDERS[it].first) })
    }
    val count = if (state.settled) "${state.items.size}${if (state.hasMore) " or more" else ""} ${if (state.items.size == 1) "photo" else "photos"}" else ""
    BasicText("", modifier = Modifier.semantics { contentDescription = count; liveRegion = LiveRegionMode.Polite })
    when {
        state.unavailable -> Alert(SEARCH_UNAVAILABLE)
        state.error != null && state.items.isEmpty() -> ErrorRetry(state.error.orEmpty(), vm::retry)
    }
    notice?.let { Alert(it, tone = AlertTone.Success) }
    when {
        state.items.isNotEmpty() -> JustifiedPhotos(
            api,
            state.items,
            actions,
            onNearEnd = vm::more,
            footer = { state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::retry) } } },
        )
        state.settled && state.error == null ->
            EmptyState(SquirrelIcon.Search, "No photos match “${state.query}”", "Try other words, or fewer filters. Photos are searched by what they show.")
        state.loading -> Loading(text = "Searching…")
        normalizeQuery(state.typed).isEmpty() && !state.unavailable ->
            EmptyState(SquirrelIcon.Sparkles, "Search by what is in your photos", "Type a few words, in any language: “dog on the beach”, “snowy mountains”.")
    }
}

@Composable
private fun DocumentSearch(api: SquirrelApi, vm: DocumentSearchViewModel) {
    RegisterLsDo("document_search", remember(vm) { DocumentSearchLsDo(vm) })
    val state by vm.state.collectAsStateWithLifecycle()
    SearchField(
        value = state.typed,
        onValueChange = vm::type,
        label = "Search files",
        placeholder = "Search your files",
        onSearch = vm::now,
        autofocus = state.typed.isEmpty(),
    )
    state.hint?.let { BasicText(it, style = SquirrelTheme.type.caption) }
    DocumentResults(api, vm, state)
}
