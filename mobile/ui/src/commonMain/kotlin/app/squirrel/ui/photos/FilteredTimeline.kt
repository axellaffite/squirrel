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
import app.squirrel.ui.explore.ExploreViewModel
import app.squirrel.ui.explore.FilterBar
import app.squirrel.ui.explore.PhotoFilter
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.viewer.ViewerHost
import app.squirrel.ui.viewer.ViewerRequest

/**
 * The timeline narrowed to [filter] (a person, place, tag or camera from Explore), titled
 * [title]; the filter bar refines it further, as on the web's `/photos?…`.
 */
@Composable
fun FilteredTimeline(graph: AppGraph, filter: PhotoFilter, title: String, onChange: (PhotoFilter) -> Unit, onBack: () -> Unit) {
    val api = graph.api ?: return
    val explore = viewModel(key = "explore") { ExploreViewModel(api) }
    val people by explore.state.collectAsStateWithLifecycle()
    val vm = viewModel(key = "timeline-${filter.key}") { PhotosViewModel(api, filter = filter) }
    var viewing by remember { mutableStateOf<String?>(null) }
    SystemBack(onBack = onBack)
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader(title, onBack = onBack, backLabel = "Back to Explore")
        FilterBar(api, filter, people.people.orEmpty(), onChange)
        TimelineList(api, vm, TileActions.open { viewing = it.id })
    }
    viewing?.let { id ->
        val state by vm.state.collectAsStateWithLifecycle()
        val items = remember(state.months) { state.loaded() }
        ViewerHost(graph, ViewerRequest(items, id, vm::near), onClose = { viewing = null })
    }
}
