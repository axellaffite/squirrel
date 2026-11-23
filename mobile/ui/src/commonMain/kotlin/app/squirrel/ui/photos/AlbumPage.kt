package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.tabular
import app.squirrel.ui.AppGraph
import app.squirrel.ui.explore.photoCount
import app.squirrel.ui.sharing.CommentsSheet
import app.squirrel.ui.sharing.ShareSheet
import app.squirrel.ui.sharing.Subject
import app.squirrel.ui.sharing.memberName
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest

private enum class AlbumSheet { Share, Comments }

/**
 * One album, one's own or shared: its photos, the viewer over them, and the tools its access
 * allows (comments for everyone, sharing for the owner). [rootLabel] names where back goes.
 */
@Composable
fun AlbumPage(graph: AppGraph, id: String, name: String, rootLabel: String, onBack: () -> Unit) {
    val api = graph.api ?: return
    val vm = viewModel(key = "album-$id") { AlbumViewModel(api, id) }
    val state by vm.state.collectAsStateWithLifecycle()
    var viewing by remember { mutableStateOf<String?>(null) }
    var sheet by remember { mutableStateOf<AlbumSheet?>(null) }
    val title = state.album?.name ?: name
    val can = state.can
    SystemBack(onBack = onBack)
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader(title, onBack = onBack, backLabel = "Back to ${rootLabel.lowercase()}", actions = {
            IconButton(SquirrelIcon.MessageCircle, label = "Comments on $title", onClick = { sheet = AlbumSheet.Comments }, tinted = true)
            if (can.owner) IconButton(SquirrelIcon.Share, label = "Share $title", onClick = { sheet = AlbumSheet.Share }, tinted = true)
        })
        state.album?.let { album ->
            val line = listOfNotNull(photoCount(album.itemCount), album.sharedBy?.let { "Shared by ${memberName(it)}" }).joinToString(" · ")
            BasicText(line, style = SquirrelTheme.type.caption.tabular())
        }
        val empty = if (can.edit) "Select photos in the timeline, then choose Add to album." else "Photos added to it show up here."
        AlbumPhotos(api, vm, TileActions.open { viewing = it.id }, emptyLine = empty)
    }
    when (sheet) {
        AlbumSheet.Share -> ShareSheet(api, Subject.Album(id), title, onClose = { sheet = null })
        AlbumSheet.Comments -> CommentsSheet(api, Subject.Album(id), "“$title”", onClose = { sheet = null })
        null -> Unit
    }
    viewing?.let { start ->
        ViewerHost(graph, ViewerRequest(state.items, start, vm::near), onClose = { viewing = null })
    }
}
