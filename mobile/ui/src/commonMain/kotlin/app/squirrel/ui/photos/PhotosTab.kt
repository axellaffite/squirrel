package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.SegmentedControl
import app.squirrel.ui.AppGraph
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack

/** The photo the viewer opened on, and its list: an album's photos, or the timeline (null). */
private data class ViewerTarget(val startId: String, val albumId: String?)

/** Photos: "Timeline | Albums", an album's page, and the viewer over either. */
@Composable
fun PhotosTab(graph: AppGraph) {
    val api = graph.api ?: return
    var mode by rememberSaveable { mutableIntStateOf(0) }
    var album by rememberSaveable { mutableStateOf<Pair<String, String>?>(null) } // id to name
    var viewer by remember { mutableStateOf<ViewerTarget?>(null) }
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        val current = album
        if (current != null) {
            SystemBack { album = null }
            PageHeader(current.second, onBack = { album = null }, backLabel = "Back to albums")
            val vm = viewModel(key = "album-${current.first}") { AlbumViewModel(api, current.first) }
            AlbumPhotos(api, vm) { viewer = ViewerTarget(it.id, current.first) }
        } else {
            PageHeader("Photos")
            SegmentedControl(listOf("Timeline", "Albums"), mode, onSelect = { mode = it })
            if (mode == 0) {
                TimelineList(api, viewModel(key = "timeline") { PhotosViewModel(api) }) { viewer = ViewerTarget(it.id, null) }
            } else {
                AlbumsGrid(api, viewModel(key = "albums") { AlbumsViewModel(api) }) { album = it.id to it.name }
            }
        }
    }
    viewer?.let { target ->
        val close = { viewer = null }
        val albumId = target.albumId
        if (albumId != null) {
            val vm = viewModel(key = "album-$albumId") { AlbumViewModel(api, albumId) }
            val state by vm.state.collectAsStateWithLifecycle()
            PhotoViewer(api, graph.files, state.items, target.startId, vm::near, close)
        } else {
            val vm = viewModel(key = "timeline") { PhotosViewModel(api) }
            val state by vm.state.collectAsStateWithLifecycle()
            val items = remember(state.months) { state.loaded() }
            PhotoViewer(api, graph.files, items, target.startId, vm::near, close)
        }
    }
}
