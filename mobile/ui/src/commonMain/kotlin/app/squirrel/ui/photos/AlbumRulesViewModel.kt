package app.squirrel.ui.photos

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.AlbumPreviewDto
import app.squirrel.api.models.AlbumRulesDto
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

data class AlbumRulesState(
    /** A new album (its name is asked here); else an existing album's rules. */
    val creating: Boolean = true,
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
 * The rule editor of a dynamic album, new ([albumId] null) or existing ([initial] its rules):
 * the pickers' choices, a debounced live preview (`POST /api/albums/preview`) and the save.
 */
class AlbumRulesViewModel(
    private val api: SquirrelApi,
    private val albumId: String?,
    initial: AlbumRulesDto? = null,
    private val debounceMillis: Long = PREVIEW_DEBOUNCE_MILLIS,
) : ViewModel() {
    private val _state = MutableStateFlow(AlbumRulesState(creating = albumId == null, form = initial?.let(RulesForm::from) ?: RulesForm()))
    val state: StateFlow<AlbumRulesState> = _state.asStateFlow()

    private var previewJob: Job? = null

    /** Bumped on every change: an older preview's answer is dropped. */
    private var generation = 0

    init {
        loadChoices()
        schedulePreview()
    }

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
        _state.update { it.copy(form = change(it.form), error = null) }
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
        if (current.saving) return null
        current.problem?.let { problem ->
            _state.update { it.copy(error = problem) }
            return null
        }
        val rules = current.form.toRules() ?: return null
        _state.update { it.copy(saving = true, error = null) }
        val result = call {
            if (albumId == null) api.albums.createAlbum(CreateAlbumRequest(name = current.name.trim(), rules = rules)).bodyOrThrow()
            else api.albums.updateAlbum(albumId, UpdateAlbumRequest(rules = rules)).bodyOrThrow()
        }
        _state.update { it.copy(saving = false, error = (result as? Outcome.Failed)?.message) }
        return (result as? Outcome.Ok)?.value
    }
}
