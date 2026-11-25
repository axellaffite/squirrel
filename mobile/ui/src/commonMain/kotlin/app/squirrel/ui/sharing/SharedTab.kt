package app.squirrel.ui.sharing

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.api.models.AssetDetails
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.NodeKind
import app.squirrel.api.models.SharedItemDto
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.Loading
import app.squirrel.ui.AppGraph
import app.squirrel.ui.files.FilesViewModel
import app.squirrel.ui.files.FolderBrowser
import app.squirrel.ui.files.OpenerConfirm
import app.squirrel.ui.photos.AlbumPage
import app.squirrel.ui.shell.Navigator
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest
import io.github.axellaffite.lsdo.compose.RegisterLsDo

/** The pages of the Shared tab. */
sealed interface SharedRoute {
    data object List : SharedRoute
    data class Folder(val id: String) : SharedRoute
    data class Album(val id: String, val name: String) : SharedRoute
    data class Photo(val id: String, val name: String) : SharedRoute

    /** A file to open with another app (from a notification): it opens, and the list shows again. */
    data class File(val node: NodeDto) : SharedRoute
}

/** Whether a shared file opens in the photo viewer (rather than with another app). */
fun isPhotoFile(mime: String?): Boolean = mime != null && (mime.startsWith("image/") || mime.startsWith("video/"))

/**
 * Shared: what other people shared with the caller. A folder, album or photo opens in the
 * normal screens, with only what its access allows.
 */
@Composable
fun SharedTab(graph: AppGraph) {
    val api = graph.api ?: return
    val nav = viewModel(key = "nav-shared") { Navigator<SharedRoute>(SharedRoute.List) }
    val vm = viewModel(key = "shared") { SharedViewModel(api, graph.files) }
    val stack by nav.stack.collectAsStateWithLifecycle()
    // LS-DO: an agent goes back through the tab's own stack, and opens rows through the list's open.
    val navLsDo = remember(nav) { SharedNavLsDo(nav) }
    LaunchedEffect(stack) { navLsDo.shown(stack.last()) }
    RegisterLsDo("shared_nav", navLsDo)
    when (val route = stack.last()) {
        SharedRoute.List -> {
            RegisterLsDo("shared_list", remember(vm, nav) { SharedListLsDo(vm) { item -> open(item, vm, nav) } })
            SharedList(api, vm) { item -> open(item, vm, nav) }
            OpenerConfirm(vm.opener)
        }
        is SharedRoute.Folder -> FolderBrowser(
            graph,
            viewModel(key = "shared-folder-${route.id}") { FilesViewModel(api, graph.files, root = route.id) },
            rootLabel = "Shared",
            onLeave = { nav.pop() },
        )
        is SharedRoute.Album -> AlbumPage(graph, route.id, route.name, rootLabel = "Shared", onBack = { nav.pop() })
        is SharedRoute.Photo -> SharedPhoto(graph, route, onBack = { nav.pop() })
        is SharedRoute.File -> LaunchedEffect(route) {
            nav.pop()
            vm.opener.open(route.node)
        }
    }
}

private fun open(item: SharedItemDto, vm: SharedViewModel, nav: Navigator<SharedRoute>) {
    item.album?.let { return nav.push(SharedRoute.Album(it.id, it.name)) }
    val node = item.node ?: return
    when {
        node.kind == NodeKind.folder -> nav.push(SharedRoute.Folder(node.id))
        isPhotoFile(node.mime) -> nav.push(SharedRoute.Photo(node.id, node.name))
        else -> vm.opener.open(node)
    }
}

/** A photo shared on its own: the viewer over just it, once its details arrived. */
@Composable
private fun SharedPhoto(graph: AppGraph, route: SharedRoute.Photo, onBack: () -> Unit) {
    val api = graph.api ?: return
    val details by produceState<Outcome<AssetDetails>?>(null, route.id) {
        value = call { api.photos.getPhoto(route.id).bodyOrThrow() }
    }
    val loaded = details
    if (loaded is Outcome.Ok) return ViewerHost(graph, ViewerRequest(listOf(loaded.value.asset), loaded.value.asset.id), onClose = onBack)
    SystemBack(onBack = onBack)
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader(route.name, onBack = onBack, backLabel = "Back to shared")
        if (loaded is Outcome.Failed) Alert(loaded.message) else Loading()
    }
}
