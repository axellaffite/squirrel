package app.squirrel.ui.photos

import app.squirrel.ui.shell.Navigator
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** The sub-tabs of Photos' first page, as an agent names them. */
@Serializable
enum class PhotosMode(val label: String) {
    @SerialName("timeline") Timeline("Timeline"),
    @SerialName("albums") Albums("Albums"),
    @SerialName("explore") Explore("Explore"),
}

/** The kinds of page Photos stacks. */
@Serializable
enum class PhotosPage {
    @SerialName("home") Home,
    @SerialName("album") Album,
    @SerialName("search") Search,
    @SerialName("section") Section,
    @SerialName("filtered") Filtered,
    @SerialName("album_rules") AlbumRules,
}

@Serializable
data class PhotosView(
    @Doc("The page shown: home (the sub-tabs), album, search, section (one Explore list) or filtered (a timeline opened from Explore) or album_rules (a new dynamic album's rules)") val page: PhotosPage = PhotosPage.Home,
    @Doc("The sub-tab of home (kept while another page is on top)") val mode: PhotosMode = PhotosMode.Timeline,
    @Doc("The album, section or filtered timeline's title; null at home and in search") val title: String? = null,
    @Doc("The album shown, on an album page") val albumId: String? = null,
    @Doc("Pages stacked, home included: back goes down one") val depth: Int = 1,
)

/** What an agent can do in the Photos section (LS-DO): its sub-tabs, search and back. */
@Serializable
sealed interface PhotosIntent {
    @Serializable @SerialName("show") @Doc("Show a sub-tab (timeline, albums, explore), as the segmented control does. Home only")
    data class Show(val mode: PhotosMode) : PhotosIntent

    @Serializable @SerialName("search") @Doc("Open photo search, as the search button does. Home only")
    data object Search : PhotosIntent

    @Serializable @SerialName("back") @Doc("Back one page, as the page's back button does. Not at home")
    data object Back : PhotosIntent
}

/**
 * Photos' own navigation as an LS-DO screen ("photos"). The stack is [nav]'s; the sub-tab stays
 * Compose state: [select] sets it as the segmented control does, and [shown] mirrors both here.
 */
class PhotosLsDo(private val nav: Navigator<PhotosRoute>, mode: PhotosMode, private val select: (PhotosMode) -> Unit) :
    LsDoScreen<PhotosView, PhotosIntent> {
    override val lsdo = lsdoSpec<PhotosView, PhotosIntent>()

    private val _state = MutableStateFlow(view(nav.stack.value, mode))
    override val state: StateFlow<PhotosView> = _state.asStateFlow()

    override suspend fun perform(intent: PhotosIntent): String? = when (intent) {
        is PhotosIntent.Show -> done { select(intent.mode) }
        PhotosIntent.Search -> done { nav.push(PhotosRoute.Search) }
        PhotosIntent.Back -> if (nav.pop()) null else "Already at the first page of Photos."
    }

    override fun availableIntents(state: PhotosView): Set<String> =
        if (state.page == PhotosPage.Home) setOf("show", "search") else setOf("back")

    /** Photos recomposed with [stack] and [mode] shown. */
    fun shown(stack: List<PhotosRoute>, mode: PhotosMode) {
        _state.value = view(stack, mode)
    }

    private fun view(stack: List<PhotosRoute>, mode: PhotosMode): PhotosView = when (val top = stack.last()) {
        PhotosRoute.Home -> PhotosView(PhotosPage.Home, mode, depth = stack.size)
        is PhotosRoute.Album -> PhotosView(PhotosPage.Album, mode, top.name, top.id, stack.size)
        PhotosRoute.Search -> PhotosView(PhotosPage.Search, mode, depth = stack.size)
        is PhotosRoute.Section -> PhotosView(PhotosPage.Section, mode, top.section.title, depth = stack.size)
        is PhotosRoute.Filtered -> PhotosView(PhotosPage.Filtered, mode, top.title, depth = stack.size)
        is PhotosRoute.NewDynamicAlbum -> PhotosView(PhotosPage.AlbumRules, mode, "New dynamic album", depth = stack.size)
    }
}
