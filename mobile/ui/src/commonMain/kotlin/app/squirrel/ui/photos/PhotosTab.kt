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
import app.squirrel.api.models.AssetSummary
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.SegmentedControl
import app.squirrel.ui.AppGraph
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack

private data class ViewerTarget(val items: List<AssetSummary>, val start: Int)

/** Photos: "Timeline | Albums", an album's page, and the viewer over either. */
@Composable
fun PhotosTab(graph: AppGraph) {
    val api = graph.api ?: return
    var mode by rememberSaveable { mutableIntStateOf(0) }
    var album by rememberSaveable { mutableStateOf<Pair<String, String>?>(null) } // id to name
    var viewer by remember { mutableStateOf<ViewerTarget?>(null) }
    val open: (List<AssetSummary>, Int) -> Unit = { items, index -> if (index >= 0) viewer = ViewerTarget(items, index) }
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        val current = album
        if (current != null) {
            SystemBack { album = null }
            PageHeader(current.second, onBack = { album = null }, backLabel = "Back to albums")
            val vm = viewModel(key = "album-${current.first}") { AlbumViewModel(api, current.first) }
            AlbumPhotos(api, vm, open)
        } else {
            PageHeader("Photos")
            SegmentedControl(listOf("Timeline", "Albums"), mode, onSelect = { mode = it })
            if (mode == 0) {
                TimelineList(api, viewModel(key = "timeline-${api.baseUrl}") { PhotosViewModel(api) }, open)
            } else {
                AlbumsGrid(api, viewModel(key = "albums-${api.baseUrl}") { AlbumsViewModel(api) }) { album = it.id to it.name }
            }
        }
    }
    viewer?.let { target -> PhotoViewer(api, graph.files, target.items, target.start) { viewer = null } }
}
