package app.squirrel.ui.photos

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.explore.PhotoFilter
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

private const val LOADED = "the timeline loaded"

/** One month of the timeline as LS-DO lists it. */
@Serializable
data class MonthRow(
    @Doc("e.g. 2024-07") val month: String,
    val label: String,
    @Doc("Photos taken that month") val count: Long,
    @Doc("Photos of it loaded so far") val loaded: Int,
    @Doc("Every photo of it is loaded") val complete: Boolean,
    val loading: Boolean = false,
    @Doc("Its last page failed: retry") val failed: Boolean = false,
)

@Serializable
data class TimelineView(
    @Doc("Set on a timeline opened from Explore: its title (a person, place, tag or camera)") val title: String? = null,
    @Doc("What the timeline is narrowed to; null: every photo") val filter: PhotoFilter? = null,
    val months: List<MonthRow> = emptyList(),
    @Doc("Every photo loaded so far, newest first; months load page by page (more)") val photos: List<PhotoRow> = emptyList(),
    val loadingMonths: Boolean = false,
    val refreshing: Boolean = false,
    val error: String? = null,
    @Doc("The photo open in the viewer") val viewing: String? = null,
    @Doc("What the last add to an album did") val notice: String? = null,
)

/** What an agent can do on the timeline (LS-DO); each maps to the call the UI makes. */
@Serializable
sealed interface TimelineIntent {
    @Serializable @SerialName("more") @Doc("Load the next page of a month, as scrolling to it does; no month: the first one not complete. Not once every month is complete")
    data class More(@Doc("e.g. 2024-07, from months") val month: String? = null) : TimelineIntent

    @Serializable @SerialName("refresh") @Doc("Reload the timeline from the top, as pulling down does")
    data object Refresh : TimelineIntent

    @Serializable @SerialName("retry") @Doc("Try the failed load again, after an error")
    data object Retry : TimelineIntent

    @Serializable @SerialName("open") @Doc("Open a loaded photo (by its id in photos) in the viewer. Not while one is open")
    data class Open(val id: String) : TimelineIntent

    @Serializable @SerialName("close_photo") @Doc("Close the viewer. Only while a photo is open (viewing set)")
    data object ClosePhoto : TimelineIntent

    @Serializable @SerialName("add_to_album")
    @Doc("Add loaded photos to an album (one of yours, or shared with you for editing), as selecting them and choosing Add to album does")
    data class AddToAlbum(@Doc("Photo ids from photos") val ids: List<String>, @Doc("An album id, as albums lists them") val albumId: String) : TimelineIntent

    @Serializable @SerialName("add_to_new_album")
    @Doc("Add loaded photos to a new album named name, as Add to album, then New album… does")
    data class AddToNewAlbum(@Doc("Photo ids from photos") val ids: List<String>, val name: String) : TimelineIntent

    @Serializable @SerialName("filter")
    @Doc("Narrow or widen a timeline opened from Explore, as its filter bar does; an empty filter goes back to Explore. Only on a timeline opened from Explore")
    data class Filter(val filter: PhotoFilter) : TimelineIntent
}

/**
 * The timeline as an LS-DO screen ("timeline"): the whole one in Photos, or one opened from
 * Explore ([title] and [onFilter] set). The viewer and the notice stay Compose state, mirrored
 * in [overlay]; [onOpen], [onClose], [onAdded] and [onFilter] change them as the UI does.
 */
class TimelineLsDo(
    private val vm: PhotosViewModel,
    private val api: SquirrelApi,
    scope: CoroutineScope,
    private val onOpen: (String) -> Unit,
    private val onClose: () -> Unit,
    /** Photos went into an album: the UI's notice and album refresh; null where photos cannot be added. */
    private val onAdded: ((String) -> Unit)? = null,
    private val title: String? = null,
    private val filter: PhotoFilter? = null,
    private val onFilter: ((PhotoFilter) -> Unit)? = null,
) : LsDoScreen<TimelineView, TimelineIntent> {
    val overlay = OverlayMirror()

    override val lsdo = lsdoSpec<TimelineView, TimelineIntent>()

    override val state: StateFlow<TimelineView> = vm.state.viewWith(overlay.state) { s, o ->
        TimelineView(
            title = title,
            filter = filter,
            months = s.months.map { MonthRow(it.month, it.label, it.count, it.items.size, it.complete, it.loading, it.failed) },
            photos = s.loaded().map { it.row() },
            loadingMonths = s.loadingMonths,
            refreshing = s.refreshing,
            error = s.error,
            viewing = o.viewing,
            notice = o.notice,
        )
    }

    override suspend fun perform(intent: TimelineIntent): String? = when (intent) {
        is TimelineIntent.More -> more(intent.month)
        TimelineIntent.Refresh -> done { vm.refresh() }
        TimelineIntent.Retry -> done { vm.retry() }
        is TimelineIntent.Open -> missingPhotos(listOf(intent.id), vm.state.value.loaded(), LOADED) ?: done { onOpen(intent.id) }
        TimelineIntent.ClosePhoto -> done(onClose)
        is TimelineIntent.AddToAlbum -> withAdded { addShownToAlbum(api, intent.ids, vm.state.value.loaded(), LOADED, intent.albumId, it) }
        is TimelineIntent.AddToNewAlbum -> withAdded { addShownToNewAlbum(api, intent.ids, vm.state.value.loaded(), LOADED, intent.name, it) }
        is TimelineIntent.Filter -> {
            val change = onFilter ?: return "Only a timeline opened from Explore has a filter."
            done { change(intent.filter.normalized()) }
        }
    }

    private fun more(month: String?): String? {
        val months = vm.state.value.months
        val page = if (month == null) months.firstOrNull { !it.complete } else months.firstOrNull { it.month == month }
        return when {
            page == null && month == null -> "Every month is loaded."
            page == null -> "No month \"$month\" in the timeline."
            page.complete -> "${page.label} is loaded."
            else -> done { vm.ensure(page.month) }
        }
    }

    private suspend fun withAdded(add: suspend ((String) -> Unit) -> String?): String? {
        val notify = onAdded ?: return "Photos cannot be added to an album from here."
        return add(notify)
    }

    override fun availableIntents(state: TimelineView): Set<String> = buildSet {
        addAll(listOf("refresh"))
        if (state.months.any { !it.complete }) add("more")
        if (state.error != null) add("retry")
        if (state.viewing == null) add("open") else add("close_photo")
        if (onAdded != null && state.viewing == null) addAll(listOf("add_to_album", "add_to_new_album"))
        if (onFilter != null) add("filter")
    }

    override fun isBusy(state: TimelineView): Boolean = state.loadingMonths || state.refreshing || state.months.any { it.loading }
}
