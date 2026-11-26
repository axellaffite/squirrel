package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.api.models.AlbumDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.SegmentedControl
import app.squirrel.ui.AppGraph
import app.squirrel.ui.explore.ExploreOverview
import app.squirrel.ui.explore.ExploreSection
import app.squirrel.ui.explore.ExploreSectionPage
import app.squirrel.ui.explore.ExploreViewModel
import app.squirrel.ui.explore.OpenFilter
import app.squirrel.ui.explore.PhotoFilter
import app.squirrel.ui.files.albumNameProblem
import app.squirrel.ui.search.SearchKind
import app.squirrel.ui.search.SearchScreen
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.shell.NameSheet
import app.squirrel.ui.shell.Navigator
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest
import io.github.axellaffite.lsdo.compose.RegisterLsDo
import kotlin.random.Random

/** The pages of the Photos tab. */
sealed interface PhotosRoute {
    data object Home : PhotosRoute
    data class Album(val id: String, val name: String) : PhotosRoute
    data object Search : PhotosRoute
    data class Section(val section: ExploreSection) : PhotosRoute
    data class Filtered(val filter: PhotoFilter, val title: String) : PhotosRoute

    /** The rule editor of a new dynamic album; [token] gives each opening its own editor. */
    data class NewDynamicAlbum(val token: Long = Random.nextLong()) : PhotosRoute
}

/** Photos: "Timeline | Albums | Explore", albums, Explore's pages, search, and the viewer. */
@Composable
fun PhotosTab(graph: AppGraph) {
    val api = graph.api ?: return
    val nav = viewModel(key = "nav-photos") { Navigator<PhotosRoute>(PhotosRoute.Home) }
    val stack by nav.stack.collectAsStateWithLifecycle()
    // Here rather than in PhotosHome, so LS-DO's "photos" screen sees and sets it from any page.
    var modeName by rememberSaveable { mutableStateOf(PhotosMode.Timeline.name) }
    val mode = PhotosMode.valueOf(modeName)
    val lsdo = remember(nav) { PhotosLsDo(nav, mode) { modeName = it.name } }
    LaunchedEffect(stack, mode) { lsdo.shown(stack, mode) }
    RegisterLsDo("photos", lsdo)
    when (val route = stack.last()) {
        PhotosRoute.Home -> PhotosHome(graph, nav, mode) { modeName = it.name }
        is PhotosRoute.Album -> AlbumPage(graph, route.id, route.name, rootLabel = "Albums", onBack = { nav.pop() })
        is PhotosRoute.NewDynamicAlbum -> {
            val vm = viewModel(key = "album-rules-new-${route.token}") { AlbumRulesViewModel(api, albumId = null) }
            val albums = viewModel(key = "albums") { AlbumsViewModel(api) }
            AlbumRulesPage(api, vm, "New dynamic album", onSaved = { album ->
                albums.refresh()
                nav.replace(PhotosRoute.Album(album.id, album.name))
            }, onBack = { nav.pop() })
        }
        PhotosRoute.Search -> SearchScreen(graph, SearchKind.Photos, onBack = { nav.pop() })
        is PhotosRoute.Section -> ExploreSectionPage(api, explore(api), route.section, onBack = { nav.pop() }, onOpen = filtered(nav))
        is PhotosRoute.Filtered -> FilteredTimeline(
            graph,
            route.filter,
            route.title,
            onChange = { next -> if (next.isEmpty) nav.pop() else nav.replace(route.copy(filter = next)) },
            onBack = { nav.pop() },
        )
    }
}

@Composable
private fun explore(api: SquirrelApi) = viewModel(key = "explore") { ExploreViewModel(api) }

private fun filtered(nav: Navigator<PhotosRoute>): OpenFilter = { filter, title -> nav.push(PhotosRoute.Filtered(filter, title)) }

@Composable
private fun PhotosHome(graph: AppGraph, nav: Navigator<PhotosRoute>, mode: PhotosMode, onMode: (PhotosMode) -> Unit) {
    val api = graph.api ?: return
    var viewing by remember { mutableStateOf<String?>(null) }
    var selected by remember { mutableStateOf(emptySet<String>()) }
    var picking by remember { mutableStateOf<List<String>?>(null) }
    var naming by remember { mutableStateOf(false) }
    var choosing by remember { mutableStateOf(false) }
    var notice by remember { mutableStateOf<String?>(null) }
    val timeline = viewModel(key = "timeline") { PhotosViewModel(api) }
    val albums = viewModel(key = "albums") { AlbumsViewModel(api) }
    val scope = rememberCoroutineScope()
    // A new sub-tab starts without a selection or a notice, however it was chosen.
    LaunchedEffect(mode) {
        selected = emptySet()
        notice = null
    }
    val onAdded: (String) -> Unit = { message ->
        notice = message
        selected = emptySet()
        albums.refresh()
    }
    SystemBack(enabled = selected.isNotEmpty()) { selected = emptySet() }
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            PageHeader("Photos", actions = {
                if (mode == PhotosMode.Albums) IconButton(SquirrelIcon.Plus, label = "New album", onClick = { choosing = true })
                IconButton(SquirrelIcon.Search, label = "Search photos", onClick = { nav.push(PhotosRoute.Search) })
            })
            SegmentedControl(PhotosMode.entries.map { it.label }, mode.ordinal, onSelect = { onMode(PhotosMode.entries[it]) })
            notice?.let { Alert(it, tone = AlertTone.Success) }
            when (mode) {
                PhotosMode.Timeline -> {
                    val lsdo = remember(timeline) {
                        TimelineLsDo(timeline, api, scope, onOpen = { viewing = it }, onClose = { viewing = null }, onAdded = onAdded)
                    }
                    LaunchedEffect(viewing, notice) { lsdo.overlay.shown(PhotosOverlay(viewing, notice)) }
                    RegisterLsDo("timeline", lsdo)
                    TimelineList(api, timeline, TileActions({ viewing = it.id }, selected) { selected = it }) {
                        item(key = "albums") {
                            AlbumStrip(api, albums, onOpen = { nav.push(PhotosRoute.Album(it.id, it.name)) }, onSeeAll = { onMode(PhotosMode.Albums) })
                        }
                    }
                }
                PhotosMode.Albums -> {
                    val open: (AlbumDto) -> Unit = { nav.push(PhotosRoute.Album(it.id, it.name)) }
                    RegisterLsDo("albums", remember(albums) { AlbumsLsDo(albums, api, scope, onNewDynamic = { nav.push(PhotosRoute.NewDynamicAlbum()) }, onOpen = open) })
                    AlbumsGrid(api, albums, open)
                }
                PhotosMode.Explore -> ExploreOverview(api, explore(api), onSeeAll = { nav.push(PhotosRoute.Section(it)) }, onOpen = filtered(nav))
            }
        }
        if (selected.isNotEmpty()) {
            SelectionBar(selected.size, onClear = { selected = emptySet() }, modifier = Modifier.align(Alignment.BottomCenter)) {
                Button("Add to album", onClick = { picking = selected.toList() }, size = ButtonSize.Sm, icon = SquirrelIcon.Library)
            }
        }
    }
    picking?.let { ids ->
        AlbumPickerSheet(api, ids, onDone = onAdded, onClose = { picking = null })
    }
    if (choosing) {
        NewAlbumSheet(
            onManual = { choosing = false; naming = true },
            onDynamic = { choosing = false; nav.push(PhotosRoute.NewDynamicAlbum()) },
            onClose = { choosing = false },
        )
    }
    if (naming) {
        NameSheet("New album", "Create album", validate = ::albumNameProblem, onSubmit = { name ->
            when (val result = createAlbum(api, name)) {
                is Outcome.Ok -> null.also {
                    albums.refresh()
                    nav.push(PhotosRoute.Album(result.value.id, result.value.name))
                }
                is Outcome.Failed -> result.message
            }
        }, onClose = { naming = false })
    }
    viewing?.let { id ->
        val state by timeline.state.collectAsStateWithLifecycle()
        val items = remember(state.months) { state.loaded() }
        ViewerHost(graph, ViewerRequest(items, id, timeline::near), onClose = { viewing = null }, onAddToAlbum = { picking = listOf(it.id) })
    }
}
