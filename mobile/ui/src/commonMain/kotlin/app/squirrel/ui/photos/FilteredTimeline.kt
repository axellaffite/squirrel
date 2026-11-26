package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.ui.AppGraph
import app.squirrel.ui.explore.ExploreViewModel
import app.squirrel.ui.explore.FilterBar
import app.squirrel.ui.explore.PhotoFilter
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest
import io.github.axellaffite.lsdo.compose.RegisterLsDo

/**
 * The timeline narrowed to [filter] (a person, place, tag or camera from Explore), titled
 * [title]; the filter bar refines it further, as on the web's `/photos?…`.
 */
@Composable
fun FilteredTimeline(graph: AppGraph, filter: PhotoFilter, title: String, onChange: (PhotoFilter) -> Unit, onBack: () -> Unit) {
    val api = graph.api ?: return
    val explore = viewModel(key = "explore") { ExploreViewModel(api) }
    val people by explore.state.collectAsStateWithLifecycle()
    val vm = viewModel(key = "timeline-${filter.key}") { PhotosViewModel(api, filter = filter) }
    val albums = viewModel(key = "albums") { AlbumsViewModel(api) }
    var viewing by remember { mutableStateOf<String?>(null) }
    var selected by remember { mutableStateOf(emptySet<String>()) }
    var notice by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val downloads = rememberPhotoDownloads(api, graph.saver)
    val onAdded: (String) -> Unit = { message ->
        notice = message
        selected = emptySet()
        albums.refresh()
    }
    val lsdo = remember(vm) {
        TimelineLsDo(vm, api, scope, onOpen = { viewing = it }, onClose = { viewing = null }, onAdded = onAdded, title = title, filter = filter, onFilter = onChange, downloads = downloads)
    }
    LaunchedEffect(viewing, notice) { lsdo.overlay.shown(PhotosOverlay(viewing, notice)) }
    RegisterLsDo("timeline", lsdo)
    SystemBack { if (selected.isNotEmpty()) selected = emptySet() else onBack() }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            PageHeader(title, onBack = onBack, backLabel = "Back to Explore")
            FilterBar(api, filter, people.people.orEmpty(), onChange)
            notice?.let { Alert(it, tone = AlertTone.Success) }
            TimelineList(api, vm, TileActions({ viewing = it.id }, selected) { selected = it })
        }
        AddToAlbumSelection(
            api,
            selected,
            onClear = { selected = emptySet() },
            onAdded = onAdded,
            modifier = Modifier.align(Alignment.BottomCenter),
            downloads = downloads,
            nameOf = { id -> vm.state.value.loaded().firstOrNull { it.id == id }?.name },
        )
    }
    viewing?.let { id ->
        val state by vm.state.collectAsStateWithLifecycle()
        val items = remember(state.months) { state.loaded() }
        ViewerHost(graph, ViewerRequest(items, id, vm::near), onClose = { viewing = null })
    }
}
