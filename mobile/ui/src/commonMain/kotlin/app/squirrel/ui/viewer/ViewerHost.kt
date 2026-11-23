package app.squirrel.ui.viewer

import androidx.compose.runtime.Composable
import app.squirrel.api.models.AssetSummary
import app.squirrel.ui.AppGraph
import app.squirrel.ui.photos.PhotoViewer

/** What the viewer pages through: [items] (growing as pages arrive, asked for with [onNear]). */
class ViewerRequest(val items: List<AssetSummary>, val startId: String, val onNear: (String) -> Unit = {})

/** The photo viewer over a list, with the tools its access allows. */
@Composable
fun ViewerHost(graph: AppGraph, request: ViewerRequest, onClose: () -> Unit) {
    val api = graph.api ?: return
    PhotoViewer(api, graph.files, request.items, request.startId, request.onNear, onClose)
}
