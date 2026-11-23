package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.theme.Tokens
import app.squirrel.ui.AppGraph
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest

/** One album: its photos, and the viewer over them. [rootLabel] names where back goes. */
@Composable
fun AlbumPage(graph: AppGraph, id: String, name: String, rootLabel: String, onBack: () -> Unit) {
    val api = graph.api ?: return
    val vm = viewModel(key = "album-$id") { AlbumViewModel(api, id) }
    val state by vm.state.collectAsStateWithLifecycle()
    var viewing by remember { mutableStateOf<String?>(null) }
    SystemBack(onBack = onBack)
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader(name, onBack = onBack, backLabel = "Back to ${rootLabel.lowercase()}")
        AlbumPhotos(api, vm, TileActions.open { viewing = it.id }, emptyLine = "Photos added to it show up here.")
    }
    viewing?.let { start ->
        ViewerHost(graph, ViewerRequest(state.items, start, vm::near), onClose = { viewing = null })
    }
}
