package app.squirrel.ui.explore

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.CameraGroupDto
import app.squirrel.api.models.PersonDto
import app.squirrel.api.models.PlaceGroupDto
import app.squirrel.api.models.PlaceLevel
import app.squirrel.api.models.TagGroupDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.ResumeThrottle
import app.squirrel.ui.attempt
import app.squirrel.ui.photos.UNREACHABLE
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** What Explore shows; a list is null until it first arrives. */
data class ExploreState(
    val people: List<PersonDto>? = null,
    val places: List<PlaceGroupDto>? = null,
    val tags: List<TagGroupDto>? = null,
    val cameras: List<CameraGroupDto>? = null,
    val loading: Boolean = false,
    val error: String? = null,
) {
    val loaded: Boolean get() = people != null && places != null && tags != null && cameras != null
    val empty: Boolean
        get() = loaded && people.orEmpty().isEmpty() && places.orEmpty().isEmpty() && tags.orEmpty().isEmpty() && cameras.orEmpty().isEmpty()
    val refreshing: Boolean get() = loading && loaded
}

/**
 * Explore: people, places, tags and cameras of the caller's photos, loaded together. A list that
 * fails keeps what it showed (or shows nothing) and the screen offers "Try again".
 */
class ExploreViewModel(private val api: SquirrelApi, private val throttle: ResumeThrottle = ResumeThrottle()) : ViewModel() {
    private val _state = MutableStateFlow(ExploreState())
    val state: StateFlow<ExploreState> = _state.asStateFlow()

    init {
        load()
    }

    fun load() {
        if (_state.value.loading) return
        throttle.loaded()
        _state.update { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                val people = async { attempt { api.people.listPeople().bodyOrThrow() } }
                val places = async { attempt { api.explore.listPlaces(PlaceLevel.city).bodyOrThrow() } }
                val tags = async { attempt { api.explore.listTags().bodyOrThrow() } }
                val cameras = async { attempt { api.explore.listCameras().bodyOrThrow() } }
                val results = listOf(people.await(), places.await(), tags.await(), cameras.await())
                _state.update { s ->
                    s.copy(
                        people = people.await() ?: s.people ?: emptyList(),
                        places = places.await() ?: s.places ?: emptyList(),
                        tags = tags.await() ?: s.tags ?: emptyList(),
                        cameras = cameras.await() ?: s.cameras ?: emptyList(),
                        error = if (results.any { it == null }) UNREACHABLE else null,
                    )
                }
            } finally {
                _state.update { it.copy(loading = false) }
            }
        }
    }

    /** The app came back: a refresh, unless Explore was loaded moments ago. */
    fun resumed() {
        if (throttle.due()) load()
    }
}
