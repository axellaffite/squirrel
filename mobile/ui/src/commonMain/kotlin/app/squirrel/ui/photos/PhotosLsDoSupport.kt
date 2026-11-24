package app.squirrel.ui.photos

import app.squirrel.api.models.AssetSummary
import app.squirrel.lsdo.Doc
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.serialization.Serializable

/** A photo as LS-DO lists it: enough to pick one and tell it apart. */
@Serializable
data class PhotoRow(
    val id: String,
    val name: String,
    @Doc("Capture time on the camera's clock, e.g. 2024-07-14T18:30:05") val takenAt: String,
    val video: Boolean = false,
)

fun AssetSummary.row(): PhotoRow = PhotoRow(id, name, takenAt, video = mime?.startsWith("video/") == true)

/**
 * What a photo screen keeps in Compose rather than in its view model: the photo open in the
 * viewer and the notice shown above the photos. The screen mirrors them here with [shown].
 */
data class PhotosOverlay(val viewing: String? = null, val notice: String? = null, val noticeIsError: Boolean = false)

/** The [PhotosOverlay] of an LS-DO screen, mirrored from Compose. */
class OverlayMirror {
    private val _state = MutableStateFlow(PhotosOverlay())
    val state: StateFlow<PhotosOverlay> = _state.asStateFlow()

    fun shown(overlay: PhotosOverlay) {
        _state.value = overlay
    }
}

/**
 * A screen's serializable view of a view model's state and its overlay, kept current in
 * [scope] (the composition's, while shown): the view model's own state is larger than an agent
 * needs and not serializable.
 */
internal fun <T, R> StateFlow<T>.viewWith(overlay: StateFlow<PhotosOverlay>, scope: CoroutineScope, transform: (T, PhotosOverlay) -> R): StateFlow<R> =
    combine(this, overlay, transform).stateIn(scope, SharingStarted.Eagerly, transform(value, overlay.value))

/** [viewWith], for a screen without an overlay. */
internal fun <T, R> StateFlow<T>.view(scope: CoroutineScope, transform: (T) -> R): StateFlow<R> =
    map(transform).stateIn(scope, SharingStarted.Eagerly, transform(value))

/** A call the screen cannot refuse. */
internal inline fun done(call: () -> Unit): String? {
    call()
    return null
}

/** Refuses ids not among the photos [shown]; null when every one is there. */
internal fun missingPhotos(ids: List<String>, shown: List<AssetSummary>, where: String): String? {
    if (ids.isEmpty()) return "Give at least one photo id."
    val known = shown.map { it.id }.toSet()
    val missing = ids.filterNot { it in known }
    return if (missing.isEmpty()) null else "No photo ${missing.joinToString { "\"$it\"" }} in $where."
}
