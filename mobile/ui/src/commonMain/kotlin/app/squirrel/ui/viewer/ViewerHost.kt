package app.squirrel.ui.viewer

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import app.squirrel.api.models.AccessLevel
import app.squirrel.api.models.AssetSummary
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.theme.SquirrelIcon
import app.squirrel.ui.AppGraph
import app.squirrel.ui.attempt
import app.squirrel.ui.photos.PhotoViewer
import app.squirrel.ui.photos.StageButton
import app.squirrel.ui.photos.dayLabel
import app.squirrel.ui.sharing.CommentsSheet
import app.squirrel.ui.sharing.ShareSheet
import app.squirrel.ui.sharing.Subject

/** What the viewer pages through: [items] (growing as pages arrive, asked for with [onNear]). */
class ViewerRequest(val items: List<AssetSummary>, val startId: String, val onNear: (String) -> Unit = {})

private sealed interface ViewerSheet {
    data class Comments(val photo: AssetSummary) : ViewerSheet
    data class Share(val photo: AssetSummary) : ViewerSheet
}

/** "the photo taken 14 July 2024, 18:30": how the viewer's buttons name their subject. */
fun takenLabel(photo: AssetSummary): String {
    val time = photo.takenAt.drop(11).take(5)
    val kind = if (photo.mime?.startsWith("video/") == true) "video" else "photo"
    return "the $kind taken ${dayLabel(photo.takenAt)}${if (time.isNotEmpty()) ", $time" else ""}"
}

/**
 * The photo viewer over a list, with the tools the photo on screen allows: comments for
 * everyone who sees it; sharing and adding to an album for its owner only, once
 * `GET /api/photos/{id}` said so. Faces and people are never asked for here.
 */
@Composable
fun ViewerHost(graph: AppGraph, request: ViewerRequest, onClose: () -> Unit, onAddToAlbum: ((AssetSummary) -> Unit)? = null) {
    val api = graph.api ?: return
    var sheet by remember { mutableStateOf<ViewerSheet?>(null) }
    PhotoViewer(api, graph.saver, request.items, request.startId, request.onNear, onClose, tools = { photo ->
        val owned = ownedBy(api, photo)
        val subject = takenLabel(photo)
        StageButton(SquirrelIcon.MessageCircle, "Comments on $subject") { sheet = ViewerSheet.Comments(photo) }
        if (owned) StageButton(SquirrelIcon.Share, "Share $subject") { sheet = ViewerSheet.Share(photo) }
        if (owned && onAddToAlbum != null) StageButton(SquirrelIcon.Library, "Add $subject to an album") { onAddToAlbum(photo) }
    })
    when (val current = sheet) {
        is ViewerSheet.Comments -> CommentsSheet(api, Subject.Node(current.photo.id), takenLabel(current.photo), onClose = { sheet = null })
        is ViewerSheet.Share -> ShareSheet(api, Subject.Node(current.photo.id), current.photo.name, onClose = { sheet = null })
        null -> Unit
    }
}

/** Whether the caller owns [photo]; false until its details say so. */
@Composable
private fun ownedBy(api: SquirrelApi, photo: AssetSummary): Boolean {
    val access by produceState<AccessLevel?>(null, photo.id) {
        value = attempt { api.photos.getPhoto(photo.id).bodyOrThrow() }?.access
    }
    return access == AccessLevel.owner
}
