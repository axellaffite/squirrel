package app.squirrel.ui.explore

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import app.squirrel.ui.RegisterLsDo
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.PullToRefresh
import app.squirrel.ui.OnForeground
import app.squirrel.ui.PullableColumn
import app.squirrel.ui.photos.ErrorRetry
import app.squirrel.ui.photos.RailClearance
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Cards shown per section before "See all". */
internal const val PREVIEW = 6

/** One list of Explore, on a page of its own. */
@Serializable
enum class ExploreSection(val title: String) {
    @SerialName("people") People("People"),
    @SerialName("places") Places("Places"),
    @SerialName("tags") Tags("Tags"),
    @SerialName("cameras") Cameras("Cameras"),
}

/** Explore's overview: a few people, places, tags and cameras, each with "See all". */
@Composable
fun ExploreOverview(api: SquirrelApi, vm: ExploreViewModel, onSeeAll: (ExploreSection) -> Unit, onOpen: OpenFilter) {
    val state by vm.state.collectAsStateWithLifecycle()
    RegisterExploreLsDo(vm, null, onSeeAll, onOpen)
    OnForeground(vm::resumed)
    when {
        !state.loaded && state.error != null -> return PullableColumn(state.loading, vm::load) { ErrorRetry(state.error.orEmpty(), vm::load) }
        !state.loaded -> return Loading()
        state.empty -> return PullableColumn(state.refreshing, vm::load) {
            state.error?.let { ErrorRetry(it, vm::load) }
            EmptyState(SquirrelIcon.Sparkles, "Nothing to explore yet", "People, places and tags appear here as your photos are analysed.")
        }
    }
    ExploreList(state.refreshing, vm) {
        state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::load) } }
        section(ExploreSection.People, state.people.orEmpty(), onSeeAll) { PeopleGrid(api, it, onOpen) }
        section(ExploreSection.Places, state.places.orEmpty(), onSeeAll) { PlaceCards(api, it, onOpen) }
        section(ExploreSection.Tags, state.tags.orEmpty(), onSeeAll) { TagCards(api, it, onOpen) }
        section(ExploreSection.Cameras, state.cameras.orEmpty(), onSeeAll) { CameraRows(it, onOpen) }
    }
}

/** Explore on show, as LS-DO's "explore" screen: the overview ([section] null) or one section's page. */
@Composable
private fun RegisterExploreLsDo(vm: ExploreViewModel, section: ExploreSection?, onSeeAll: (ExploreSection) -> Unit, onOpen: OpenFilter) {
    val scope = rememberCoroutineScope()
    RegisterLsDo("explore", remember(vm, section) { ExploreLsDo(vm, section, scope, onSeeAll, onOpen) })
}

private fun <T> LazyListScope.section(
    which: ExploreSection,
    all: List<T>,
    onSeeAll: (ExploreSection) -> Unit,
    content: @Composable (List<T>) -> Unit,
) {
    if (all.isEmpty()) return
    item(key = which.name) {
        Column(verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
            SectionHead(which.title, if (all.size > PREVIEW) ({ onSeeAll(which) }) else null)
            content(all.take(PREVIEW))
        }
    }
}

@Composable
private fun ExploreList(refreshing: Boolean, vm: ExploreViewModel, content: LazyListScope.() -> Unit) {
    PullToRefresh(refreshing, vm::load) {
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(Tokens.space3),
            contentPadding = PaddingValues(bottom = RailClearance),
            content = content,
        )
    }
}

/** Every person, place, tag or camera: one section of Explore on its own page. */
@Composable
fun ExploreSectionPage(api: SquirrelApi, vm: ExploreViewModel, section: ExploreSection, onBack: () -> Unit, onOpen: OpenFilter) {
    val state by vm.state.collectAsStateWithLifecycle()
    RegisterExploreLsDo(vm, section, onSeeAll = {}, onOpen = onOpen)
    SystemBack(onBack = onBack)
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader(section.title, onBack = onBack, backLabel = "Back to Explore")
        ExploreList(state.refreshing, vm) {
            state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::load) } }
            item(key = "list") {
                Column(verticalArrangement = Arrangement.spacedBy(Tokens.space4)) { SectionList(api, state, section, onOpen) }
            }
        }
    }
}

@Composable
private fun SectionList(api: SquirrelApi, state: ExploreState, section: ExploreSection, onOpen: OpenFilter) {
                when (section) {
                    ExploreSection.People -> PeopleGrid(api, state.people.orEmpty(), onOpen)
                    ExploreSection.Places -> placesByCountry(state.places.orEmpty()).forEach { (country, places) ->
                        Column(verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
                            BasicText(country, style = SquirrelTheme.type.sectionTitle, modifier = Modifier.semantics { heading() })
                            PlaceCards(api, places, onOpen)
                        }
                    }
                    ExploreSection.Tags -> TagCards(api, state.tags.orEmpty(), onOpen)
                    ExploreSection.Cameras -> CameraRows(state.cameras.orEmpty(), onOpen)
                }
}
