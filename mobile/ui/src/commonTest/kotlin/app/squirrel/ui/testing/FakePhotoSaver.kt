package app.squirrel.ui.testing

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.photos.PhotoSaver
import app.squirrel.ui.photos.SaveResult
import kotlinx.coroutines.CompletableDeferred

/**
 * A [PhotoSaver] that records what it was asked to save. [gate], when set, holds each save after
 * reporting [progress], so a test can look at the state while a download runs.
 */
class FakePhotoSaver(
    var original: SaveResult = SaveResult.Saved("Pictures"),
    var archive: SaveResult = SaveResult.Saved("Downloads"),
    private val progress: Float = 0.5f,
) {
    data class Original(val id: String, val name: String, val mime: String?)
    data class Archive(val url: String, val name: String)

    val originals = mutableListOf<Original>()
    val archives = mutableListOf<Archive>()
    var gate: CompletableDeferred<Unit>? = null

    val saver: PhotoSaver = object : PhotoSaver {
        override suspend fun saveOriginal(api: SquirrelApi, id: String, name: String, mime: String?, onProgress: (Float) -> Unit): SaveResult {
            originals += Original(id, name, mime)
            onProgress(progress)
            gate?.await()
            return original
        }

        override suspend fun saveArchive(api: SquirrelApi, url: String, name: String, onProgress: (Float) -> Unit): SaveResult {
            archives += Archive(url, name)
            onProgress(progress)
            gate?.await()
            return archive
        }
    }
}
