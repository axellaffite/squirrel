package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.SegmentedControl
import app.squirrel.ui.AppGraph
import app.squirrel.ui.search.SearchKind
import app.squirrel.ui.search.SearchScreen
import app.squirrel.ui.shell.Navigator
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest

/** The pages of the Photos tab. */
sealed interface PhotosRoute {
    data object Home : PhotosRoute
    data class Album(val id: String, val name: String) : PhotosRoute
    data object Search : PhotosRoute
}

private val MODES = listOf("Timeline", "Albums")

/** Photos: "Timeline | Albums", an album's page, search, and the viewer over them. */
@Composable
fun PhotosTab(graph: AppGraph) {
    val api = graph.api ?: return
    val nav = viewModel(key = "nav-photos") { Navigator<PhotosRoute>(PhotosRoute.Home) }
    val stack by nav.stack.collectAsStateWithLifecycle()
    when (val route = stack.last()) {
        PhotosRoute.Home -> PhotosHome(graph, nav)
        is PhotosRoute.Album -> AlbumPage(graph, route.id, route.name, rootLabel = "Albums", onBack = { nav.pop() })
        PhotosRoute.Search -> SearchScreen(graph, SearchKind.Photos, onBack = { nav.pop() })
    }
}

@Composable
private fun PhotosHome(graph: AppGraph, nav: Navigator<PhotosRoute>) {
    val api = graph.api ?: return
    var mode by rememberSaveable { mutableStateOf(0) }
    var viewing by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader("Photos", actions = {
            IconButton(SquirrelIcon.Search, label = "Search photos", onClick = { nav.push(PhotosRoute.Search) })
        })
        SegmentedControl(MODES, mode, onSelect = { mode = it })
        when (mode) {
            0 -> TimelineList(api, viewModel(key = "timeline") { PhotosViewModel(api) }, TileActions.open { viewing = it.id })
            else -> AlbumsGrid(api, viewModel(key = "albums") { AlbumsViewModel(api) }) { nav.push(PhotosRoute.Album(it.id, it.name)) }
        }
    }
    viewing?.let { id ->
        val vm = viewModel(key = "timeline") { PhotosViewModel(api) }
        val state by vm.state.collectAsStateWithLifecycle()
        val items = remember(state.months) { state.loaded() }
        ViewerHost(graph, ViewerRequest(items, id, vm::near), onClose = { viewing = null })
    }
}
