package app.squirrel.ui.photos

import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.MatchMode
import app.squirrel.api.models.PhotoKindFilter
import app.squirrel.ui.explore.cameraLabel
import app.squirrel.ui.explore.countryName
import app.squirrel.ui.explore.personLabel
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class RuleChoice(
    @Doc("What the action takes: a person id, a tag, a country code or a camera make") val id: String,
    val label: String,
    @Doc("A place's region (empty: none) or a camera's model") val sub: String? = null,
    @Doc("A place's city") val city: String? = null,
)

@Serializable
data class AlbumRulesView(
    @Doc("A new dynamic album (its name is set here); else an album's rules or the blur filter being edited") val creating: Boolean,
    val name: String = "",
    @Doc("What the photos must show, matched as photo search does; empty: no search") val search: String = "",
    @Doc("Person ids chosen") val people: List<String> = emptyList(),
    @Doc("Whether photos need any or all of the people") val peopleMatch: MatchMode = MatchMode.any,
    val tags: List<String> = emptyList(),
    val tagsMatch: MatchMode = MatchMode.any,
    @Doc("Country code; null: anywhere") val country: String? = null,
    @Doc("Region of the country; empty: places without a region") val region: String? = null,
    val city: String? = null,
    val cameraMake: String? = null,
    val cameraModel: String? = null,
    @Doc("photo or screenshot; null: both") val kind: PhotoKindFilter? = null,
    @Doc("First capture day as typed (YYYY-MM-DD, inclusive)") val takenFrom: String = "",
    @Doc("Last capture day as typed (YYYY-MM-DD, inclusive)") val takenTo: String = "",
    @Doc("What stops a save, or null") val problem: String? = null,
    @Doc("Photos matching the rules now (the live preview); null while incomplete or loading") val matching: Long? = null,
    val previewing: Boolean = false,
    val previewError: String? = null,
    val saving: Boolean = false,
    val error: String? = null,
    @Doc("People that can be chosen (toggle_person)") val personChoices: List<RuleChoice> = emptyList(),
    @Doc("Tags that can be chosen (toggle_tag)") val tagChoices: List<RuleChoice> = emptyList(),
    @Doc("Countries and cities (set_place: id is the country, sub the region, city the city)") val placeChoices: List<RuleChoice> = emptyList(),
    @Doc("Cameras (set_camera: id is the make, sub the model)") val cameraChoices: List<RuleChoice> = emptyList(),
)

/** What an agent can do in the rule editor of a dynamic album (LS-DO). */
@Serializable
sealed interface AlbumRulesIntent {
    @Serializable @SerialName("set_name") @Doc("Type the new album's name. Creating only")
    data class SetName(val name: String) : AlbumRulesIntent

    @Serializable @SerialName("set_search") @Doc("Type what the photos must show (any words, any language; at most 200 characters); empty: no search")
    data class SetSearch(val text: String) : AlbumRulesIntent

    @Serializable @SerialName("toggle_person") @Doc("Add or remove a person (an id of personChoices); at most 20")
    data class TogglePerson(val id: String) : AlbumRulesIntent

    @Serializable @SerialName("toggle_tag") @Doc("Add or remove a tag (of tagChoices); at most 20")
    data class ToggleTag(val tag: String) : AlbumRulesIntent

    @Serializable @SerialName("match") @Doc("Whether photos need any or all of the people (group people) or tags (group tags)")
    data class Match(val group: String, val mode: MatchMode) : AlbumRulesIntent

    @Serializable @SerialName("set_place") @Doc("Only photos taken there; region empty: places without a region")
    data class SetPlace(val country: String, val region: String? = null, val city: String? = null) : AlbumRulesIntent

    @Serializable @SerialName("clear_place") @Doc("Any place")
    data object ClearPlace : AlbumRulesIntent

    @Serializable @SerialName("set_camera") @Doc("Only photos from this camera make (and model)")
    data class SetCamera(val make: String, val model: String? = null) : AlbumRulesIntent

    @Serializable @SerialName("clear_camera") @Doc("Any camera")
    data object ClearCamera : AlbumRulesIntent

    @Serializable @SerialName("set_kind") @Doc("photo, screenshot, or null for both")
    data class SetKind(val kind: PhotoKindFilter? = null) : AlbumRulesIntent

    @Serializable @SerialName("set_dates") @Doc("The capture days, YYYY-MM-DD, both included; empty: open-ended")
    data class SetDates(val from: String = "", val to: String = "") : AlbumRulesIntent

    @Serializable @SerialName("save") @Doc("Create the album (it opens), save the rules (back to the album), or save the blur filter, as the button does")
    data object Save : AlbumRulesIntent

    @Serializable @SerialName("cancel") @Doc("Leave without saving, as back does")
    data object Cancel : AlbumRulesIntent
}

internal fun rulesView(s: AlbumRulesState): AlbumRulesView {
    val f = s.form
    return AlbumRulesView(
        creating = s.creating,
        name = s.name,
        search = f.search,
        people = f.people,
        peopleMatch = f.peopleMatch,
        tags = f.tags,
        tagsMatch = f.tagsMatch,
        country = f.country,
        region = f.region,
        city = f.city,
        cameraMake = f.cameraMake,
        cameraModel = f.cameraModel,
        kind = f.kind,
        takenFrom = f.takenFrom,
        takenTo = f.takenTo,
        problem = s.problem,
        matching = s.preview?.count,
        previewing = s.previewing,
        previewError = s.previewError,
        saving = s.saving,
        error = s.error,
        personChoices = s.people.map { RuleChoice(it.id, personLabel(it.name)) },
        tagChoices = s.tags.map { RuleChoice(it.tag, app.squirrel.ui.explore.tagLabel(it.tag)) },
        placeChoices = s.places.map { RuleChoice(it.countryCode, countryName(it)) } +
            s.cities.map { RuleChoice(it.countryCode, listOfNotNull(it.city, countryName(it)).joinToString(", "), it.region ?: "", it.city) },
        cameraChoices = cameraChoices(s.cameras).map { RuleChoice(it.make, cameraLabel(it), it.model) },
    )
}

/**
 * The rule editor as an LS-DO screen ("album_rules"): a dynamic album's, or the blur filter's.
 * [save] does what the page's save button does, true once saved.
 */
class AlbumRulesLsDo(
    private val vm: AlbumRulesViewModel,
    private val onCancel: () -> Unit,
    private val save: suspend () -> Boolean,
) : LsDoScreen<AlbumRulesView, AlbumRulesIntent> {
    /** A dynamic album's editor: [onSaved] gets the album, as after the button. */
    constructor(vm: AlbumRulesViewModel, onSaved: (AlbumDto) -> Unit, onCancel: () -> Unit) :
        this(vm, onCancel, save = { vm.save()?.also(onSaved) != null })

    override val lsdo = lsdoSpec<AlbumRulesView, AlbumRulesIntent>()

    override val state: StateFlow<AlbumRulesView> = vm.state.view(::rulesView)

    override suspend fun perform(intent: AlbumRulesIntent): String? = when (intent) {
        is AlbumRulesIntent.SetName -> if (vm.state.value.creating) done { vm.setName(intent.name) } else "Rename the album from its page."
        is AlbumRulesIntent.SetSearch -> done { vm.edit { it.copy(search = intent.text) } }
        is AlbumRulesIntent.TogglePerson -> toggle(intent.id, vm.state.value.form.people, MAX_RULE_PEOPLE, "people") { it.togglePerson(intent.id) }
        is AlbumRulesIntent.ToggleTag -> toggle(intent.tag, vm.state.value.form.tags, MAX_RULE_TAGS, "tags") { it.toggleTag(intent.tag) }
        is AlbumRulesIntent.Match -> when (intent.group) {
            "people" -> done { vm.edit { it.copy(peopleMatch = intent.mode) } }
            "tags" -> done { vm.edit { it.copy(tagsMatch = intent.mode) } }
            else -> "group is people or tags."
        }
        is AlbumRulesIntent.SetPlace -> done { vm.edit { it.withPlace(intent.country, intent.region, intent.city) } }
        AlbumRulesIntent.ClearPlace -> done { vm.edit { it.withoutPlace() } }
        is AlbumRulesIntent.SetCamera -> done { vm.edit { it.withCamera(intent.make, intent.model) } }
        AlbumRulesIntent.ClearCamera -> done { vm.edit { it.withoutCamera() } }
        is AlbumRulesIntent.SetKind -> done { vm.edit { it.copy(kind = intent.kind) } }
        is AlbumRulesIntent.SetDates -> done { vm.edit { it.copy(takenFrom = intent.from, takenTo = intent.to) } }
        AlbumRulesIntent.Save -> if (save()) null else vm.state.value.error ?: "Not saved."
        AlbumRulesIntent.Cancel -> done(onCancel)
    }

    private fun toggle(value: String, chosen: List<String>, max: Int, what: String, change: (RulesForm) -> RulesForm): String? =
        if (value !in chosen && chosen.size >= max) "At most $max $what." else done { vm.edit(change) }

    override fun availableIntents(state: AlbumRulesView): Set<String> = buildSet {
        addAll(listOf("set_search", "toggle_person", "toggle_tag", "match", "set_place", "set_camera", "set_kind", "set_dates", "cancel"))
        if (state.creating) add("set_name")
        if (state.country != null) add("clear_place")
        if (state.cameraMake != null) add("clear_camera")
        if (state.problem == null && !state.saving) add("save")
    }

    override fun isBusy(state: AlbumRulesView): Boolean = state.previewing || state.saving
}
