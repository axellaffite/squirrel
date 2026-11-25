package app.squirrel.ui.files

import app.squirrel.ui.shell.Navigator
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** The pages of the Files section, as an agent names them. */
@Serializable
enum class FilesPage {
    @SerialName("browse") Browse,
    @SerialName("search") Search,
}

@Serializable
data class FilesNavState(
    @Doc("The page shown: the folder browser (screen files) or search (screen document_search)") val page: FilesPage,
)

/** Moving between the Files section's pages (LS-DO): the folder browser and search. */
@Serializable
sealed interface FilesNavIntent {
    @Serializable @SerialName("search") @Doc("Open search, as the folder header's search button does. Only on the folder browser")
    data object Search : FilesNavIntent

    @Serializable @SerialName("back") @Doc("Close search, back to the folder browser where it was. Only on search")
    data object Back : FilesNavIntent
}

private fun pageOf(route: FilesRoute): FilesPage = when (route) {
    FilesRoute.Browse -> FilesPage.Browse
    FilesRoute.Search -> FilesPage.Search
}

/**
 * The Files section's pages as an LS-DO screen ("files_nav"). The stack stays the tab's
 * [Navigator]: [perform] moves it as the buttons do, and [shown] mirrors it back here.
 */
class FilesNavLsDo(private val nav: Navigator<FilesRoute>) : LsDoScreen<FilesNavState, FilesNavIntent> {
    override val lsdo = lsdoSpec<FilesNavState, FilesNavIntent>()

    private val _state = MutableStateFlow(FilesNavState(pageOf(nav.top)))
    override val state: StateFlow<FilesNavState> = _state.asStateFlow()

    override suspend fun perform(intent: FilesNavIntent): String? = when (intent) {
        FilesNavIntent.Search -> if (nav.top == FilesRoute.Search) "Search is already open." else null.also { nav.push(FilesRoute.Search) }
        FilesNavIntent.Back -> if (nav.pop()) null else "Already on the folder browser."
    }.also { shown(nav.top) }

    override fun availableIntents(state: FilesNavState): Set<String> = when (state.page) {
        FilesPage.Browse -> setOf("search")
        FilesPage.Search -> setOf("back")
    }

    /** The tab recomposed with [route] on top. */
    fun shown(route: FilesRoute) {
        _state.value = FilesNavState(pageOf(route))
    }
}
