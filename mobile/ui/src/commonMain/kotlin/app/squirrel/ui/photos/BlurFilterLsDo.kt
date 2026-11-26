package app.squirrel.ui.photos

import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class BlurFilterView(
    @Doc("GET /api/me/blur-filter has not answered yet") val loading: Boolean = false,
    val loadError: String? = null,
    @Doc("Blur matching photos (the switch); off keeps the rules") val enabled: Boolean = true,
    @Doc("A filter is saved on the server (remove is offered)") val saved: Boolean = false,
    @Doc("Photos the rules being edited match now (the live count); the rules themselves are album_rules'") val matching: Long? = null,
    @Doc("What stops a save, or null") val problem: String? = null,
    val saving: Boolean = false,
    val removing: Boolean = false,
    @Doc("What the last save, switch or removal did") val notice: String? = null,
    val error: String? = null,
)

/** What an agent can do on the blur filter's page (LS-DO); the rules are edited through album_rules. */
@Serializable
sealed interface BlurFilterIntent {
    @Serializable @SerialName("set_enabled")
    @Doc("Turn Blur matching photos on or off, as the switch does: saved at once once a filter exists, else with the first save")
    data class SetEnabled(val enabled: Boolean) : BlurFilterIntent

    @Serializable @SerialName("save") @Doc("Save the rules (album_rules' state) as the blur filter, as Save filter does")
    data object Save : BlurFilterIntent

    @Serializable @SerialName("remove") @Doc("Forget the filter (nothing is blurred any more), as Remove filter does. Only once saved")
    data object Remove : BlurFilterIntent

    @Serializable @SerialName("back") @Doc("Leave the page, as its back button does (unsaved rules are dropped)")
    data object Back : BlurFilterIntent
}

internal fun blurFilterView(s: AlbumRulesState): BlurFilterView {
    val f = s.filter ?: FilterState(loading = false)
    return BlurFilterView(
        loading = f.loading,
        loadError = f.loadError,
        enabled = f.enabled,
        saved = f.saved != null,
        matching = s.preview?.count,
        problem = s.problem,
        saving = s.saving,
        removing = f.removing,
        notice = f.notice,
        error = s.error,
    )
}

/** The blur filter's page as an LS-DO screen ("blur_filter"). */
class BlurFilterLsDo(private val vm: AlbumRulesViewModel, private val onBack: () -> Unit) : LsDoScreen<BlurFilterView, BlurFilterIntent> {
    override val lsdo = lsdoSpec<BlurFilterView, BlurFilterIntent>()

    override val state: StateFlow<BlurFilterView> = vm.state.view(::blurFilterView)

    override suspend fun perform(intent: BlurFilterIntent): String? = when (intent) {
        is BlurFilterIntent.SetEnabled -> if (vm.setFilterEnabled(intent.enabled)) null else vm.state.value.error ?: "Not now: the page is busy."
        BlurFilterIntent.Save -> if (vm.saveFilter()) null else vm.state.value.error ?: "Not saved."
        BlurFilterIntent.Remove -> if (vm.removeFilter()) null else vm.state.value.error ?: "No filter to remove."
        BlurFilterIntent.Back -> done(onBack)
    }

    override fun availableIntents(state: BlurFilterView): Set<String> = buildSet {
        add("back")
        if (state.loading || state.saving || state.removing) return@buildSet
        add("set_enabled")
        if (state.problem == null) add("save")
        if (state.saved) add("remove")
    }

    override fun isBusy(state: BlurFilterView): Boolean = state.loading || state.saving || state.removing
}

@Serializable
data class BlurredView(
    @Doc("The server's list has arrived (before: every photo blurred if the filter was active last time)") val loaded: Boolean = false,
    @Doc("Photos the blur filter blurs") val count: Int = 0,
    @Doc("Node ids the blur filter blurs") val ids: List<String> = emptyList(),
    @Doc("Show blurred is on: every photo shows, until the app restarts") val revealAll: Boolean = false,
    @Doc("Photos shown one by one (the viewer's Show), until the app restarts") val revealed: List<String> = emptyList(),
)

/** What an agent can do with the blurred photos (LS-DO), anywhere once signed in. */
@Serializable
sealed interface BlurredIntent {
    @Serializable @SerialName("show_all") @Doc("Show every blurred photo until the app restarts, as the timeline's Show blurred does. Only when some are blurred")
    data object ShowAll : BlurredIntent

    @Serializable @SerialName("blur_again") @Doc("Blur them all again (forgets the one-by-one Show too), as Blur again does. Only while shown")
    data object BlurAgain : BlurredIntent

    @Serializable @SerialName("show") @Doc("Show one blurred photo, as the viewer's Show does")
    data class Show(@Doc("A node id of ids") val id: String) : BlurredIntent

    @Serializable @SerialName("refresh") @Doc("Ask the server for the list again, as coming back to the app does (without its throttle)")
    data object Refresh : BlurredIntent
}

/** The blurred photos as an LS-DO screen ("blurred"). */
class BlurredLsDo(private val blurred: BlurredPhotos) : LsDoScreen<BlurredView, BlurredIntent> {
    override val lsdo = lsdoSpec<BlurredView, BlurredIntent>()

    override val state: StateFlow<BlurredView> = blurred.state.view { s ->
        val ids = s.ids.orEmpty().toList()
        BlurredView(loaded = s.ids != null, count = ids.size, ids = ids, revealAll = s.revealAll, revealed = s.revealed.toList())
    }

    override suspend fun perform(intent: BlurredIntent): String? = when (intent) {
        BlurredIntent.ShowAll -> done { blurred.setRevealAll(true) }
        BlurredIntent.BlurAgain -> done { blurred.setRevealAll(false) }
        is BlurredIntent.Show ->
            if (blurred.state.value.ids?.contains(intent.id) == true) done { blurred.reveal(intent.id) } else "Photo ${intent.id} is not blurred."
        BlurredIntent.Refresh -> done { blurred.refresh(force = true) }
    }

    override fun availableIntents(state: BlurredView): Set<String> = buildSet {
        add("refresh")
        if (state.count > 0 && !state.revealAll) addAll(listOf("show_all", "show"))
        if (state.revealAll) add("blur_again")
    }
}
