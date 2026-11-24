package app.squirrel.ui.sharing

import app.squirrel.lsdo.Doc
import app.squirrel.lsdo.LsDoScreen
import app.squirrel.lsdo.lsdoSpec
import app.squirrel.ui.shell.Navigator
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** The pages of the Shared section, as an agent names them. */
@Serializable
enum class SharedPage {
    @SerialName("list") List,
    @SerialName("folder") Folder,
    @SerialName("album") Album,
    @SerialName("photo") Photo,
}

@Serializable
data class SharedNavState(
    @Doc("The page shown: the list (screen shared_list), a shared folder (screen shared), an album or a photo") val page: SharedPage,
    @Doc("The folder, album or photo open; null on the list") val id: String? = null,
    @Doc("The album or photo's name; null on the list and in a folder (its state has the name)") val name: String? = null,
)

/** Moving between the Shared section's pages (LS-DO); opening is the list's own open. */
@Serializable
sealed interface SharedNavIntent {
    @Serializable @SerialName("back")
    @Doc("Back to the Shared list from what is open, a folder at any depth included (as its \"Shared\" breadcrumb does). Not on the list")
    data object Back : SharedNavIntent
}

internal fun sharedNavState(route: SharedRoute): SharedNavState = when (route) {
    // A file route only lasts until the file opens with another app, over the list.
    SharedRoute.List, is SharedRoute.File -> SharedNavState(SharedPage.List)
    is SharedRoute.Folder -> SharedNavState(SharedPage.Folder, route.id)
    is SharedRoute.Album -> SharedNavState(SharedPage.Album, route.id, route.name)
    is SharedRoute.Photo -> SharedNavState(SharedPage.Photo, route.id, route.name)
}

/**
 * The Shared section's pages as an LS-DO screen ("shared_nav"). The stack stays the tab's
 * [Navigator]: [perform] moves it as the back button does, and [shown] mirrors it back here.
 */
class SharedNavLsDo(private val nav: Navigator<SharedRoute>) : LsDoScreen<SharedNavState, SharedNavIntent> {
    override val lsdo = lsdoSpec<SharedNavState, SharedNavIntent>()

    private val _state = MutableStateFlow(sharedNavState(nav.top))
    override val state: StateFlow<SharedNavState> = _state.asStateFlow()

    override suspend fun perform(intent: SharedNavIntent): String? = when (intent) {
        SharedNavIntent.Back -> if (nav.pop()) null.also { shown(nav.top) } else "Already on the Shared list."
    }

    override fun availableIntents(state: SharedNavState): Set<String> = if (state.page == SharedPage.List) emptySet() else setOf("back")

    /** The tab recomposed with [route] on top. */
    fun shown(route: SharedRoute) {
        _state.value = sharedNavState(route)
    }
}
