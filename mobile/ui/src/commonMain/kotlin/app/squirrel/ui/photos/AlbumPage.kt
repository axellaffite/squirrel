package app.squirrel.ui.photos

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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.tabular
import app.squirrel.ui.AppGraph
import app.squirrel.ui.explore.photoCount
import app.squirrel.ui.files.albumNameProblem
import app.squirrel.ui.sharing.CommentsSheet
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.ShareSheet
import app.squirrel.ui.sharing.Subject
import app.squirrel.ui.sharing.memberName
import app.squirrel.ui.shell.NameSheet
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest
import io.github.axellaffite.lsdo.compose.RegisterLsDo
import kotlinx.coroutines.launch

private enum class AlbumSheet { Share, Comments, Rename }

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
    var selected by remember { mutableStateOf(emptySet<String>()) }
    var busy by remember { mutableStateOf(false) }
    var notice by remember { mutableStateOf<Pair<String, Boolean>?>(null) } // text, is an error
    val scope = rememberCoroutineScope()
    val title = state.album?.name ?: name
    val can = state.can
    // Others' albums: writers add and remove their own photos; the rest stays with the owner.
    val selection = if (can.edit) TileActions({ viewing = it.id }, selected) { selected = selected.toggled(it.id) } else TileActions.open { viewing = it.id }
    val run: (suspend () -> Pair<String, Boolean>?) -> Unit = { work ->
        busy = true
        scope.launch {
            notice = work()
            busy = false
        }
    }
    val lsdo = remember(vm) {
        AlbumLsDo(vm, api, id, name, scope, onOpen = { viewing = it }, onClose = { viewing = null }, onNotice = { text, error ->
            selected = emptySet()
            notice = text to error
        })
    }
    LaunchedEffect(viewing, notice) { lsdo.overlay.shown(PhotosOverlay(viewing, notice?.first, notice?.second == true)) }
    RegisterLsDo("album", lsdo)
    SystemBack { if (selected.isNotEmpty()) selected = emptySet() else onBack() }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            PageHeader(title, onBack = onBack, backLabel = "Back to ${rootLabel.lowercase()}", actions = {
                IconButton(SquirrelIcon.MessageCircle, label = "Comments on $title", onClick = { sheet = AlbumSheet.Comments }, tinted = true)
                if (can.owner) {
                    IconButton(SquirrelIcon.Share, label = "Share $title", onClick = { sheet = AlbumSheet.Share }, tinted = true)
                    IconButton(SquirrelIcon.Pencil, label = "Rename the album $title", onClick = { sheet = AlbumSheet.Rename }, tinted = true)
                }
            })
            state.album?.let { album ->
                val line = listOfNotNull(photoCount(album.itemCount), album.sharedBy?.let { "Shared by ${memberName(it)}" }).joinToString(" · ")
                BasicText(line, style = SquirrelTheme.type.caption.tabular())
            }
            notice?.let { (text, error) -> Alert(text, tone = if (error) AlertTone.Error else AlertTone.Success) }
            val empty = if (can.edit) "Select photos in the timeline, then choose Add to album." else "Photos added to it show up here."
            AlbumPhotos(api, vm, selection, emptyLine = empty)
        }
        if (selected.isNotEmpty() && can.edit) {
            SelectionBar(selected.size, onClear = { selected = emptySet() }, modifier = Modifier.align(Alignment.BottomCenter)) {
                if (selected.size == 1 && can.owner) {
                    Button("Set as cover", enabled = !busy, size = ButtonSize.Sm, variant = ButtonVariant.Secondary, onClick = {
                        run {
                            when (val result = setAlbumCover(api, id, selected.first())) {
                                is Outcome.Ok -> ("Cover changed." to false).also { selected = emptySet(); vm.details() }
                                is Outcome.Failed -> result.message to true
                            }
                        }
                    })
                }
                Button("Remove", enabled = !busy, size = ButtonSize.Sm, variant = ButtonVariant.Danger, onClick = {
                    run {
                        val result = removeFromAlbum(api, id, selected.toList())
                        selected = emptySet()
                        vm.refresh()
                        when (result) {
                            is Outcome.Ok -> removedMessage(result.value) to (result.value == 0L)
                            is Outcome.Failed -> result.message to true
                        }
                    }
                })
            }
        }
    }
    when (sheet) {
        AlbumSheet.Share -> ShareSheet(api, Subject.Album(id), title, onClose = { sheet = null })
        AlbumSheet.Comments -> CommentsSheet(api, Subject.Album(id), "“$title”", onClose = { sheet = null })
        AlbumSheet.Rename -> NameSheet("Rename album", "Save", initial = title, validate = ::albumNameProblem, onSubmit = { next ->
            when (val result = renameAlbum(api, id, next)) {
                is Outcome.Ok -> null.also { vm.details() }
                is Outcome.Failed -> result.message
            }
        }, onClose = { sheet = null })
        null -> Unit
    }
    viewing?.let { start ->
        ViewerHost(graph, ViewerRequest(state.items, start, vm::near), onClose = { viewing = null })
    }
}
