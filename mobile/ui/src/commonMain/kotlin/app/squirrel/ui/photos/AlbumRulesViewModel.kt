package app.squirrel.ui.photos

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.AlbumPreviewDto
import app.squirrel.api.models.AlbumRulesDto
import app.squirrel.api.models.BlurFilterDto
import app.squirrel.api.models.CameraGroupDto
import app.squirrel.api.models.CreateAlbumRequest
import app.squirrel.api.models.PersonDto
import app.squirrel.api.models.PlaceGroupDto
import app.squirrel.api.models.PlaceLevel
import app.squirrel.api.models.TagGroupDto
import app.squirrel.api.models.UpdateAlbumRequest
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.attempt
import app.squirrel.ui.files.albumNameProblem
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.call
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** How long the rules rest before their preview is asked for. */
const val PREVIEW_DEBOUNCE_MILLIS = 400L

/** What the rules editor edits: a new dynamic album, an album's rules, or the blur filter. */
sealed interface RulesTarget {
    data object NewAlbum : RulesTarget
    data class Album(val id: String) : RulesTarget

    /** The caller's blur filter (`/api/me/blur-filter`): the photos it matches show blurred. */
    data object BlurFilter : RulesTarget
}

/** The blur filter's own part of the editor (the rules are the form's). */
data class FilterState(
    /** `GET /api/me/blur-filter` has not answered yet. */
    val loading: Boolean = true,
    val loadError: String? = null,
    /** "Blur matching photos": off keeps the rules. */
    val enabled: Boolean = true,
    /** The rules on the server; null: no filter saved. */
    val saved: AlbumRulesDto? = null,
    val removing: Boolean = false,
    /** What the last save or removal did ("Blur filter saved."). */
    val notice: String? = null,
)

data class AlbumRulesState(
    /** A new album (its name is asked here); else an existing album's rules, or the blur filter. */
    val creating: Boolean = true,
    /** The blur filter's state; null when editing an album. */
    val filter: FilterState? = null,
    val name: String = "",
    val form: RulesForm = RulesForm(),
    /** The pickers' choices: visible people, tags, countries, cities and cameras. */
    val people: List<PersonDto> = emptyList(),
    val tags: List<TagGroupDto> = emptyList(),
    val places: List<PlaceGroupDto> = emptyList(),
    val cities: List<PlaceGroupDto> = emptyList(),
    val cameras: List<CameraGroupDto> = emptyList(),
    val choicesLoaded: Boolean = false,
    /** Some choices could not be loaded (the rest still work). */
    val choicesFailed: Boolean = false,
    /** What the rules match now; null while they are incomplete or before the first answer. */
    val preview: AlbumPreviewDto? = null,
    val previewing: Boolean = false,
    val previewError: String? = null,
    val saving: Boolean = false,
    val error: String? = null,
) {
    /** What stops a save: the name (when creating), then the rules. */
    val problem: String? get() = (if (creating) albumNameProblem(name) else null) ?: form.problem
}

/**
 * The rule editor of a dynamic album, new or existing ([initial] its rules), or of the blur filter
 * (its rules loaded here): the pickers' choices, a debounced live preview
 * (`POST /api/albums/preview`, the same rules) and the save.
 */
class AlbumRulesViewModel(
    private val api: SquirrelApi,
    private val target: RulesTarget,
    initial: AlbumRulesDto? = null,
    private val debounceMillis: Long = PREVIEW_DEBOUNCE_MILLIS,
    /** The blur filter was saved or removed (the blurred photos are asked for again). */
    private val onFilterChanged: () -> Unit = {},
) : ViewModel() {
    /** A dynamic album's editor: new when [albumId] is null. */
    constructor(api: SquirrelApi, albumId: String?, initial: AlbumRulesDto? = null, debounceMillis: Long = PREVIEW_DEBOUNCE_MILLIS) :
        this(api, albumId?.let { RulesTarget.Album(it) } ?: RulesTarget.NewAlbum, initial, debounceMillis)

    private val _state = MutableStateFlow(
        AlbumRulesState(
            creating = target == RulesTarget.NewAlbum,
            filter = if (target == RulesTarget.BlurFilter) FilterState() else null,
            form = initial?.let(RulesForm::from) ?: RulesForm(),
        ),
    )
    val state: StateFlow<AlbumRulesState> = _state.asStateFlow()

    private var previewJob: Job? = null

    /** Bumped on every change: an older preview's answer is dropped. */
    private var generation = 0

    init {
        loadChoices()
        if (target == RulesTarget.BlurFilter) loadFilter() else schedulePreview()
    }

    private fun loadFilter() {
        viewModelScope.launch {
            when (val result = call { api.photos.getBlurFilter().bodyOrThrow() }) {
                is Outcome.Ok -> {
                    val dto = result.value
                    _state.update {
                        it.copy(
                            form = dto.rules?.let(RulesForm::from) ?: it.form,
                            filter = FilterState(loading = false, enabled = dto.rules == null || dto.enabled, saved = dto.rules),
                        )
                    }
                    schedulePreview()
                }
                is Outcome.Failed -> updateFilter { it.copy(loading = false, loadError = result.message) }
            }
        }
    }

    private fun updateFilter(change: (FilterState) -> FilterState) =
        _state.update { s -> s.filter?.let { s.copy(filter = change(it)) } ?: s }

    private fun loadChoices() {
        viewModelScope.launch {
            val people = async { attempt { api.people.listPeople().bodyOrThrow() } }
            val tags = async { attempt { api.explore.listTags().bodyOrThrow() } }
            val countries = async { attempt { api.explore.listPlaces(PlaceLevel.country).bodyOrThrow() } }
            val cities = async { attempt { api.explore.listPlaces(PlaceLevel.city).bodyOrThrow() } }
            val cameras = async { attempt { api.explore.listCameras().bodyOrThrow() } }
            val all = listOf(people.await(), tags.await(), countries.await(), cities.await(), cameras.await())
            _state.update {
                it.copy(
                    people = people.await().orEmpty().filter { p -> !p.hidden },
                    tags = tags.await().orEmpty(),
                    places = countries.await().orEmpty(),
                    cities = cities.await().orEmpty(),
                    cameras = cameras.await().orEmpty(),
                    choicesLoaded = true,
                    choicesFailed = all.any { list -> list == null },
                )
            }
        }
    }

    fun setName(name: String) = _state.update { it.copy(name = name, error = null) }

    /** Changes the rules; the preview follows once they rest. */
    fun edit(change: (RulesForm) -> RulesForm) {
        _state.update { s -> s.copy(form = change(s.form), error = null, filter = s.filter?.copy(notice = null)) }
        schedulePreview()
    }

    private fun schedulePreview() {
        previewJob?.cancel()
        val gen = ++generation
        val rules = _state.value.form.toRules()
        if (rules == null) {
            _state.update { it.copy(preview = null, previewing = false, previewError = null) }
            return
        }
        _state.update { it.copy(previewing = true, previewError = null) }
        previewJob = viewModelScope.launch {
            if (debounceMillis > 0) delay(debounceMillis)
            val result = call { api.albums.previewAlbum(rules).bodyOrThrow() }
            if (gen != generation) return@launch
            _state.update {
                when (result) {
                    is Outcome.Ok -> it.copy(preview = result.value, previewing = false)
                    is Outcome.Failed -> it.copy(previewing = false, previewError = result.message)
                }
            }
        }
    }

    /** Creates the album or saves its new rules: the album, or null (and [AlbumRulesState.error] says why). */
    suspend fun save(): AlbumDto? {
        val current = _state.value
        if (current.saving || target == RulesTarget.BlurFilter) return null
        current.problem?.let { problem ->
            _state.update { it.copy(error = problem) }
            return null
        }
        val rules = current.form.toRules() ?: return null
        _state.update { it.copy(saving = true, error = null) }
        val result = call {
            when (target) {
                is RulesTarget.Album -> api.albums.updateAlbum(target.id, UpdateAlbumRequest(rules = rules)).bodyOrThrow()
                else -> api.albums.createAlbum(CreateAlbumRequest(name = current.name.trim(), rules = rules)).bodyOrThrow()
            }
        }
        _state.update { it.copy(saving = false, error = (result as? Outcome.Failed)?.message) }
        return (result as? Outcome.Ok)?.value
    }

    /**
     * "Blur matching photos": once a filter is saved, the switch applies at once (with the saved
     * rules, not the ones being edited); before that, it is saved with the first rules.
     */
    suspend fun setFilterEnabled(enabled: Boolean): Boolean {
        val filter = _state.value.filter ?: return false
        if (filter.loading || filter.removing || _state.value.saving) return false
        val saved = filter.saved ?: run {
            updateFilter { it.copy(enabled = enabled, notice = null) }
            return true
        }
        return putFilter(BlurFilterDto(enabled = enabled, rules = saved), if (enabled) "Matching photos are blurred." else "Blur filter off: photos show normally.")
    }

    /** Saves the rules as the blur filter: false (and [AlbumRulesState.error] says why) when refused. */
    suspend fun saveFilter(): Boolean {
        val current = _state.value
        val filter = current.filter ?: return false
        if (current.saving || filter.loading || filter.removing) return false
        current.form.problem?.let { problem ->
            _state.update { it.copy(error = problem) }
            return false
        }
        val rules = current.form.toRules() ?: return false
        return putFilter(BlurFilterDto(enabled = filter.enabled, rules = rules), "Blur filter saved.")
    }

    /** Forgets the blur filter (`rules: null`): nothing is blurred any more. */
    suspend fun removeFilter(): Boolean {
        val current = _state.value
        val filter = current.filter ?: return false
        if (current.saving || filter.removing || filter.saved == null) return false
        updateFilter { it.copy(removing = true, notice = null) }
        _state.update { it.copy(error = null) }
        val result = call { api.photos.updateBlurFilter(BlurFilterDto(enabled = false, rules = null)).bodyOrThrow() }
        when (result) {
            is Outcome.Ok -> {
                _state.update {
                    it.copy(form = RulesForm(), filter = FilterState(loading = false, notice = "Blur filter removed: no photo is blurred."))
                }
                schedulePreview()
                onFilterChanged()
            }
            is Outcome.Failed -> {
                updateFilter { it.copy(removing = false) }
                _state.update { it.copy(error = result.message) }
            }
        }
        return result is Outcome.Ok
    }

    private suspend fun putFilter(dto: BlurFilterDto, notice: String): Boolean {
        _state.update { it.copy(saving = true, error = null) }
        updateFilter { it.copy(notice = null) }
        val result = call { api.photos.updateBlurFilter(dto).bodyOrThrow() }
        when (result) {
            is Outcome.Ok -> {
                val kept = result.value
                updateFilter { it.copy(enabled = kept.enabled, saved = kept.rules, notice = notice) }
                _state.update { it.copy(saving = false) }
                onFilterChanged()
            }
            is Outcome.Failed -> _state.update { it.copy(saving = false, error = result.message) }
        }
        return result is Outcome.Ok
    }
}
