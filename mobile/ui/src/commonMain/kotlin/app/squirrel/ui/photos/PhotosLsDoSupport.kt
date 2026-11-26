package app.squirrel.ui.photos

import app.squirrel.api.models.AssetSummary
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.files.albumNameProblem
import app.squirrel.ui.sharing.Outcome
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.combineState
import io.github.axellaffite.lsdo.mapState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.map
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
data class PhotosOverlay(
    val viewing: String? = null,
    val notice: String? = null,
    val noticeIsError: Boolean = false,
    /** The screen's Download; set by [withDownloads], null where the screen cannot download. */
    val download: DownloadState? = null,
)

/** The [PhotosOverlay] of an LS-DO screen, mirrored from Compose. */
class OverlayMirror {
    private val _state = MutableStateFlow(PhotosOverlay())
    val state: StateFlow<PhotosOverlay> = _state.asStateFlow()

    fun shown(overlay: PhotosOverlay) {
        _state.value = overlay
    }
}

/**
 * A screen's serializable view of a view model's state and its overlay, computed when read
 * ([combineState]): the view model's own state is larger than an agent needs and not serializable.
 */
internal fun <T, R> StateFlow<T>.viewWith(overlay: StateFlow<PhotosOverlay>, transform: (T, PhotosOverlay) -> R): StateFlow<R> =
    combineState(this, overlay, transform)

/** [viewWith], for a screen without an overlay. */
internal fun <T, R> StateFlow<T>.view(transform: (T) -> R): StateFlow<R> = mapState(transform)

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

/**
 * Adds [ids], each among the photos [shown] (described as [where]), to the album [albumId], one of
 * one's own or shared for writing, as the album picker does; [onAdded] hears the notice. Null once
 * they are in, else why not.
 */
internal suspend fun addShownToAlbum(
    api: SquirrelApi,
    ids: List<String>,
    shown: List<AssetSummary>,
    where: String,
    albumId: String,
    onAdded: (String) -> Unit,
): String? {
    missingPhotos(ids, shown, where)?.let { return it }
    val picked = pickerAlbums(api)
    val album = picked.albums.firstOrNull { it.id == albumId }
        ?: return if (picked.ok) "No album \"$albumId\" you can add to." else UNREACHABLE
    return added(AlbumAdder(api, ids).into(album), onAdded)
}

/** [addShownToAlbum], into a new album named [name], as the picker's "New album…" does. */
internal suspend fun addShownToNewAlbum(
    api: SquirrelApi,
    ids: List<String>,
    shown: List<AssetSummary>,
    where: String,
    name: String,
    onAdded: (String) -> Unit,
): String? {
    missingPhotos(ids, shown, where)?.let { return it }
    albumNameProblem(name)?.let { return it }
    return added(AlbumAdder(api, ids).intoNew(name.trim()), onAdded)
}

private fun added(outcome: Outcome<String>, onAdded: (String) -> Unit): String? = when (outcome) {
    is Outcome.Ok -> done { onAdded(outcome.value) }
    is Outcome.Failed -> outcome.message
}
