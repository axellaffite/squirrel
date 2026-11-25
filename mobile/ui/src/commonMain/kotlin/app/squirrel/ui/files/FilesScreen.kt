package app.squirrel.ui.files

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.ui.AppGraph
import app.squirrel.ui.search.SearchKind
import app.squirrel.ui.search.SearchScreen
import app.squirrel.ui.shell.Navigator
import io.github.axellaffite.lsdo.compose.RegisterLsDo

/** The pages of the Files tab. */
sealed interface FilesRoute {
    data object Browse : FilesRoute
    data object Search : FilesRoute
}

/** Files: the folder browser, and search. */
@Composable
fun FilesTab(graph: AppGraph) {
    val nav = viewModel(key = "nav-files") { Navigator<FilesRoute>(FilesRoute.Browse) }
    val stack by nav.stack.collectAsStateWithLifecycle()
    // LS-DO: an agent opens and closes search through the tab's own stack.
    val navLsDo = remember(nav) { FilesNavLsDo(nav) }
    LaunchedEffect(stack) { navLsDo.shown(stack.last()) }
    RegisterLsDo("files_nav", navLsDo)
    when (stack.last()) {
        FilesRoute.Browse -> {
            val api = graph.api ?: return
            FolderBrowser(graph, viewModel(key = "files") { FilesViewModel(api, graph.files) }, rootLabel = "Files", onSearch = { nav.push(FilesRoute.Search) })
        }
        FilesRoute.Search -> SearchScreen(graph, SearchKind.Files, onBack = { nav.pop() })
    }
}


