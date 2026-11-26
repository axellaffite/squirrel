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
import androidx.compose.runtime.produceState
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
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.AppGraph
import app.squirrel.ui.attempt
import app.squirrel.ui.explore.photoCount
import app.squirrel.ui.files.albumNameProblem
import app.squirrel.ui.sharing.CommentsSheet
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.ShareSheet
import app.squirrel.ui.sharing.Subject
import app.squirrel.ui.sharing.memberName
import app.squirrel.theme.components.Sheet
import app.squirrel.ui.shell.ActionsSheet
import app.squirrel.ui.shell.NameSheet
import app.squirrel.ui.shell.SheetAction
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest
import io.github.axellaffite.lsdo.compose.RegisterLsDo
import kotlinx.coroutines.launch

private enum class AlbumSheet { Share, Comments, More, Rename, Delete }

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
    // Each opening of the rule editor gets its own (a cancelled edit leaves nothing behind).
    var editingRules by remember { mutableStateOf(0) }
    val scope = rememberCoroutineScope()
    val downloads = rememberPhotoDownloads(api, graph.saver)
    val title = state.album?.name ?: name
    val can = state.can
    val dynamic = state.album?.dynamic == true
    val rules = state.album?.rules
    // Others' albums: writers add and remove their own photos; the rest stays with the owner.
    // Everyone may select, to download. Otherwise a dynamic album's photos come from its rules: its owner
    // selects one only to make it the cover.
    val selectable = (if (dynamic) can.owner else can.edit) || downloads != null
    val selection = if (selectable) TileActions({ viewing = it.id }, selected) { selected = it } else TileActions.open { viewing = it.id }
    if (editingRules > 0 && rules != null) {
        val editor = viewModel(key = "album-rules-$id-$editingRules") { AlbumRulesViewModel(api, id, rules) }
        AlbumRulesPage(api, editor, "Rules of $title", onSaved = {
            editingRules = 0
            notice = "Rules saved." to false
            vm.refresh()
        }, onBack = { editingRules = 0 })
        return
    }
    val people by produceState<Map<String, String?>>(emptyMap(), rules?.people) {
        if (!rules?.people.isNullOrEmpty()) {
            attempt { api.people.listPeople(includeHidden = true).bodyOrThrow() }?.let { list -> value = list.associate { it.id to it.name } }
        }
    }
    val albums = viewModel(key = "albums") { AlbumsViewModel(api) }
    var deleting by remember { mutableStateOf(false) }
    // Deleted: the list forgets it and the page closes; failed: the page says why. Once at a time.
    val removeAlbum: suspend () -> Pair<String, Boolean>? = remove@{
        if (deleting) return@remove "The album is already being deleted." to true
        deleting = true
        val result = try {
            deleteAlbum(api, id)
        } finally {
            deleting = false
        }
        sheet = null
        when (result) {
            is Outcome.Ok -> null.also {
                albums.refresh()
                onBack()
            }
            is Outcome.Failed -> result.message to true
        }
    }
    val run: (suspend () -> Pair<String, Boolean>?) -> Unit = { work ->
        busy = true
        scope.launch {
            notice = work()
            busy = false
        }
    }
    val lsdo = remember(vm) {
        AlbumLsDo(vm, api, id, name, scope, onOpen = { viewing = it }, onClose = { viewing = null }, downloads = downloads, onNotice = { text, error ->
            selected = emptySet()
            notice = text to error
        }, onEditRules = { editingRules += 1 }, onDelete = { removeAlbum()?.first })
    }
    LaunchedEffect(viewing, notice) { lsdo.overlay.shown(PhotosOverlay(viewing, notice?.first, notice?.second == true)) }
    RegisterLsDo("album", lsdo)
    SystemBack { if (selected.isNotEmpty()) selected = emptySet() else onBack() }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            PageHeader(title, onBack = onBack, backLabel = "Back to ${rootLabel.lowercase()}", actions = {
                IconButton(SquirrelIcon.MessageCircle, label = "Comments on $title", onClick = { sheet = AlbumSheet.Comments }, tinted = true)
                if (can.owner) {
                    if (dynamic && rules != null) {
                        IconButton(SquirrelIcon.SlidersHorizontal, label = "Edit the rules of $title", onClick = { editingRules += 1 }, tinted = true)
                    }
                    IconButton(SquirrelIcon.Share, label = "Share $title", onClick = { sheet = AlbumSheet.Share }, tinted = true)
                    IconButton(SquirrelIcon.More, label = "More for $title", onClick = { sheet = AlbumSheet.More }, tinted = true)
                }
            })
            state.album?.let { album ->
                val line = listOfNotNull(albumMeta(album), album.sharedBy?.let { "Shared by ${memberName(it)}" }).joinToString(" · ")
                BasicText(line, style = SquirrelTheme.type.caption.tabular())
                album.rules?.let { BasicText(rulesSummary(it) { person -> people[person] }, style = SquirrelTheme.type.caption) }
            }
            notice?.let { (text, error) -> Alert(text, tone = if (error) AlertTone.Error else AlertTone.Success) }
            val empty = when {
                dynamic -> DYNAMIC_EMPTY_LINE
                can.edit -> "Select photos in the timeline, then choose Add to album."
                else -> "Photos added to it show up here."
            }
            // A dynamic album reads as a timeline: its photos come by capture date, newest first.
            AlbumPhotos(api, vm, selection, emptyLine = empty, dated = dynamic)
        }
        if (selected.isNotEmpty() && selectable) {
            SelectionBar(
                selected.size,
                onClear = { selected = emptySet() },
                modifier = Modifier.align(Alignment.BottomCenter),
                status = { DownloadStatus(downloads) },
            ) {
                downloads?.let { d -> DownloadAction(d, selected) { photo -> state.items.firstOrNull { it.id == photo }?.name } }
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
                if (can.edit && !dynamic) {
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
    }
    DownloadConfirm(downloads)
    when (sheet) {
        AlbumSheet.Share -> ShareSheet(api, Subject.Album(id), title, onClose = { sheet = null }, readOnly = dynamic)
        AlbumSheet.Comments -> CommentsSheet(api, Subject.Album(id), "“$title”", onClose = { sheet = null })
        AlbumSheet.More -> ActionsSheet(
            title,
            listOf(
                SheetAction("Rename", SquirrelIcon.Pencil, "Rename the album $title") { sheet = AlbumSheet.Rename },
                SheetAction("Delete album", SquirrelIcon.Trash, "Delete the album $title", danger = true) { sheet = AlbumSheet.Delete },
            ),
            onClose = { if (sheet == AlbumSheet.More) sheet = null },
        )
        AlbumSheet.Delete -> Sheet("Delete “$title”?", onDismiss = { sheet = null }) {
            BasicText("The album goes for everyone it is shared with. Its photos stay in the timeline.", style = SquirrelTheme.type.body)
            Button("Delete album", enabled = !busy, onClick = { run { removeAlbum() } }, variant = ButtonVariant.DangerSolid, full = true)
            Button("Cancel", onClick = { sheet = null }, variant = ButtonVariant.Quiet, full = true)
        }
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
