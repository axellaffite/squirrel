package app.squirrel.ui.search

import app.squirrel.api.models.SearchOrder
import app.squirrel.lsdo.Doc
import app.squirrel.lsdo.LsDoScreen
import app.squirrel.lsdo.lsdoSpec
import app.squirrel.ui.explore.PhotoFilter
import app.squirrel.ui.explore.personLabel
import app.squirrel.ui.photos.PhotoRow
import app.squirrel.ui.photos.OverlayMirror
import app.squirrel.ui.photos.done
import app.squirrel.ui.photos.missingPhotos
import app.squirrel.ui.photos.row
import app.squirrel.ui.photos.viewWith
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** A person a search can be narrowed to. */
@Serializable
data class PersonChoice(val id: String, val name: String)

@Serializable
data class PhotoSearchView(
    @Doc("What is in the search field") val typed: String = "",
    @Doc("The query the photos belong to; empty before any search") val query: String = "",
    val filter: PhotoFilter = PhotoFilter(),
    val order: SearchOrder = SearchOrder.date,
    @Doc("The photos found so far") val photos: List<PhotoRow> = emptyList(),
    @Doc("More photos match: more loads them") val hasMore: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
    @Doc("This server cannot search photos (no text model): nothing to retry") val unavailable: Boolean = false,
    @Doc("Your people, for filter.person") val people: List<PersonChoice> = emptyList(),
    @Doc("The photo open in the viewer") val viewing: String? = null,
)

/** What an agent can do in photo search (LS-DO); closing the search is photos back. */
@Serializable
sealed interface PhotoSearchIntent {
    @Serializable @SerialName("search") @Doc("Search photos by what they show, as typing in the field then the search key does; empty text clears")
    data class Search(val text: String) : PhotoSearchIntent

    @Serializable @SerialName("filter") @Doc("Narrow the results, as the filter bar does; searches again at once")
    data class Filter(val filter: PhotoFilter) : PhotoSearchIntent

    @Serializable @SerialName("order") @Doc("Order the results by date (Timeline) or relevance (Best match). Not when search is unavailable")
    data class Order(val order: SearchOrder) : PhotoSearchIntent

    @Serializable @SerialName("more") @Doc("Load the next results, as scrolling to the end does. Only when hasMore")
    data object More : PhotoSearchIntent

    @Serializable @SerialName("retry") @Doc("Try again, after an error")
    data object Retry : PhotoSearchIntent

    @Serializable @SerialName("open") @Doc("Open a photo found (by its id in photos) in the viewer. Not while one is open")
    data class Open(val id: String) : PhotoSearchIntent

    @Serializable @SerialName("close_photo") @Doc("Close the viewer. Only while a photo is open (viewing set)")
    data object ClosePhoto : PhotoSearchIntent
}

/** Photo search as an LS-DO screen ("photo_search"). The viewer stays Compose state, mirrored in [overlay]. */
class PhotoSearchLsDo(
    private val vm: PhotoSearchViewModel,
    scope: CoroutineScope,
    private val onOpen: (String) -> Unit,
    private val onClose: () -> Unit,
) : LsDoScreen<PhotoSearchView, PhotoSearchIntent> {
    val overlay = OverlayMirror()

    override val lsdo = lsdoSpec<PhotoSearchView, PhotoSearchIntent>()

    override val state: StateFlow<PhotoSearchView> = vm.state.viewWith(overlay.state) { s, o ->
        PhotoSearchView(
            typed = s.typed,
            query = s.query,
            filter = s.filter,
            order = s.order,
            photos = s.items.map { it.row() },
            hasMore = s.hasMore,
            loading = s.loading,
            error = s.error,
            unavailable = s.unavailable,
            people = s.people.map { PersonChoice(it.id, personLabel(it.name)) },
            viewing = o.viewing,
        )
    }

    override suspend fun perform(intent: PhotoSearchIntent): String? = when (intent) {
        is PhotoSearchIntent.Search -> done {
            vm.type(intent.text)
            vm.now()
        }
        is PhotoSearchIntent.Filter -> done { vm.setFilter(intent.filter) }
        is PhotoSearchIntent.Order ->
            if (intent.order == SearchOrder.unknown_default_open_api) "Order by date or relevance." else done { vm.setOrder(intent.order) }
        PhotoSearchIntent.More -> done { vm.more() }
        PhotoSearchIntent.Retry -> done { vm.retry() }
        is PhotoSearchIntent.Open -> missingPhotos(listOf(intent.id), vm.state.value.items, "the results") ?: done { onOpen(intent.id) }
        PhotoSearchIntent.ClosePhoto -> done(onClose)
    }

    override fun availableIntents(state: PhotoSearchView): Set<String> = buildSet {
        addAll(listOf("search", "filter"))
        if (!state.unavailable) add("order")
        if (state.hasMore && !state.loading) add("more")
        if (state.error != null && !state.unavailable) add("retry")
        if (state.viewing == null) add("open") else add("close_photo")
    }

    override fun isBusy(state: PhotoSearchView): Boolean = state.loading
}
