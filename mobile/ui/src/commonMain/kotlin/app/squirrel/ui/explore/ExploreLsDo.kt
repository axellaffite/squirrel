package app.squirrel.ui.explore

import app.squirrel.ui.photos.done
import app.squirrel.ui.photos.view
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** A card of Explore: a person, place, tag or camera, opened by its [id]. */
@Serializable
data class ExploreRow(
    @Doc("What open takes: person:<id>, place:<country>/<region>/<city>, tag:<slug> or camera:<make>/<model>") val id: String,
    val name: String,
    @Doc("Photos it has") val count: Long,
)

@Serializable
data class ExploreView(
    @Doc("The list shown on its own page (after see_all); null: the overview, a few of each") val section: ExploreSection? = null,
    @Doc("The cards shown") val people: List<ExploreRow> = emptyList(),
    val places: List<ExploreRow> = emptyList(),
    val tags: List<ExploreRow> = emptyList(),
    val cameras: List<ExploreRow> = emptyList(),
    @Doc("Sections with more cards than the overview shows: see_all lists them") val more: List<ExploreSection> = emptyList(),
    @Doc("Every list has arrived once") val loaded: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
)

/** What an agent can do on Explore (LS-DO); back from a section page is photos back. */
@Serializable
sealed interface ExploreIntent {
    @Serializable @SerialName("open") @Doc("Open the timeline of a card shown (by its id), as tapping it does")
    data class Open(val id: String) : ExploreIntent

    @Serializable @SerialName("see_all") @Doc("Show every card of a section on its own page, as See all does. Overview only, for a section in more")
    data class SeeAll(val section: ExploreSection) : ExploreIntent

    @Serializable @SerialName("refresh") @Doc("Reload every list, as pulling down does")
    data object Refresh : ExploreIntent

    @Serializable @SerialName("retry") @Doc("Try the failed load again, after an error")
    data object Retry : ExploreIntent
}

/** A card shown: its row, and what opening it does. */
private class Card(val row: ExploreRow, val filter: PhotoFilter)

/**
 * Explore as an LS-DO screen ("explore"): the overview, or one [section] on its own page. Only
 * the cards on screen are listed and can be opened; [onOpen] and [onSeeAll] are the UI's own.
 */
class ExploreLsDo(
    private val vm: ExploreViewModel,
    private val section: ExploreSection?,
    scope: CoroutineScope,
    private val onSeeAll: (ExploreSection) -> Unit,
    private val onOpen: OpenFilter,
) : LsDoScreen<ExploreView, ExploreIntent> {
    override val lsdo = lsdoSpec<ExploreView, ExploreIntent>()

    override val state: StateFlow<ExploreView> = vm.state.view { s ->
        val cards = cards(s)
        ExploreView(
            section = section,
            people = cards.getValue(ExploreSection.People).map { it.row },
            places = cards.getValue(ExploreSection.Places).map { it.row },
            tags = cards.getValue(ExploreSection.Tags).map { it.row },
            cameras = cards.getValue(ExploreSection.Cameras).map { it.row },
            more = if (section == null) ExploreSection.entries.filter { all(s, it).size > PREVIEW } else emptyList(),
            loaded = s.loaded,
            loading = s.loading,
            error = s.error,
        )
    }

    override suspend fun perform(intent: ExploreIntent): String? = when (intent) {
        is ExploreIntent.Open -> {
            val card = cards(vm.state.value).values.flatten().firstOrNull { it.row.id == intent.id } ?: return "No card \"${intent.id}\" shown on Explore."
            done { onOpen(card.filter, card.row.name) }
        }
        is ExploreIntent.SeeAll -> if (intent.section in state.value.more) done { onSeeAll(intent.section) } else "${intent.section.title} has no more to show."
        ExploreIntent.Refresh, ExploreIntent.Retry -> done { vm.load() }
    }

    /** The cards on screen per section: a few of each on the overview, all of one on its page. */
    private fun cards(s: ExploreState): Map<ExploreSection, List<Card>> = ExploreSection.entries.associateWith { which ->
        when {
            section == null -> all(s, which).take(PREVIEW)
            section == which -> all(s, which)
            else -> emptyList()
        }
    }

    /** Each card's id, name and filter, as ExploreCards builds them. */
    private fun all(s: ExploreState, which: ExploreSection): List<Card> = when (which) {
        ExploreSection.People -> s.people.orEmpty().map { Card(ExploreRow("person:${it.id}", personLabel(it.name), it.faceCount), PhotoFilter(person = it.id)) }
        // Their own page groups places by country.
        ExploreSection.Places -> (if (section == which) placesByCountry(s.places.orEmpty()).flatMap { it.second } else s.places.orEmpty()).map {
            Card(ExploreRow("place:${it.countryCode}/${it.region.orEmpty()}/${it.city.orEmpty()}", placeName(it), it.count), placeFilter(it))
        }
        ExploreSection.Tags -> s.tags.orEmpty().map { Card(ExploreRow("tag:${it.tag}", tagLabel(it.tag), it.count), PhotoFilter(tag = it.tag)) }
        ExploreSection.Cameras -> s.cameras.orEmpty().map {
            Card(ExploreRow("camera:${it.make}/${it.model.orEmpty()}", cameraLabel(it), it.count), PhotoFilter(cameraMake = it.make, cameraModel = it.model))
        }
    }

    override fun availableIntents(state: ExploreView): Set<String> = buildSet {
        add("refresh")
        if (listOf(state.people, state.places, state.tags, state.cameras).any { it.isNotEmpty() }) add("open")
        if (state.more.isNotEmpty()) add("see_all")
        if (state.error != null) add("retry")
    }

    override fun isBusy(state: ExploreView): Boolean = state.loading
}
