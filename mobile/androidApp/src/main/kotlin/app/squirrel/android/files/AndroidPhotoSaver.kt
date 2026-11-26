package app.squirrel.android.files

import android.content.ContentResolver
import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Environment
import android.provider.MediaStore
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.files.safeFileName
import app.squirrel.ui.files.streamDownload
import app.squirrel.ui.photos.PhotoSaver
import app.squirrel.ui.photos.SaveResult
import app.squirrel.ui.photos.UNREACHABLE
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.IOException

private const val ZIP = "application/zip"
private const val ANY = "application/octet-stream"
private const val FOLDER = "Squirrel"

/** Where a download lands in shared storage, and what the message calls it. */
private enum class Place(val label: String, val collection: () -> Uri, val relativePath: String) {
    Pictures("Pictures", { MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY) }, "${Environment.DIRECTORY_PICTURES}/$FOLDER"),
    Movies("Movies", { MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY) }, "${Environment.DIRECTORY_MOVIES}/$FOLDER"),
    Downloads("Downloads", { MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY) }, Environment.DIRECTORY_DOWNLOADS),
    ;

    companion object {
        /** Images and videos go where the gallery shows them; anything else to Downloads. */
        fun of(mime: String?): Place = when {
            mime?.startsWith("image/") == true -> Pictures
            mime?.startsWith("video/") == true -> Movies
            else -> Downloads
        }
    }
}

/** The phone could not write what was downloaded (storage full, MediaStore refused it). */
private class NotKept(cause: Throwable? = null) : Exception(cause)

/**
 * Saves downloads into shared storage through MediaStore (scoped storage, minimum SDK 29: no
 * permission needed): photos in Pictures/Squirrel, videos in Movies/Squirrel, zips in Downloads.
 * Each item stays pending (hidden from other apps) until its last byte is written, and is deleted
 * when the download fails.
 */
class AndroidPhotoSaver(context: Context) : PhotoSaver {
    private val resolver: ContentResolver = context.applicationContext.contentResolver

    override suspend fun saveOriginal(api: SquirrelApi, id: String, name: String, mime: String?, onProgress: (Float) -> Unit): SaveResult {
        val place = Place.of(mime)
        return save(api, api.contentUrl(id), name, if (place == Place.Downloads) mime ?: ANY else mime.orEmpty(), place, onProgress)
    }

    override suspend fun saveArchive(api: SquirrelApi, url: String, name: String, onProgress: (Float) -> Unit): SaveResult =
        save(api, url, name, ZIP, Place.Downloads, onProgress)

    private suspend fun save(api: SquirrelApi, url: String, name: String, mime: String, place: Place, onProgress: (Float) -> Unit): SaveResult {
        var item: Uri? = null
        try {
            withContext(Dispatchers.IO) {
                val uri = insert(name, mime, place).also { item = it }
                val out = resolver.openOutputStream(uri) ?: throw NotKept()
                out.use { stream ->
                    streamDownload(api, url, onProgress) { buffer, n ->
                        try {
                            stream.write(buffer, 0, n)
                        } catch (e: IOException) {
                            throw NotKept(e)
                        }
                    }
                }
                resolver.update(uri, ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }, null, null)
            }
        } catch (e: CancellationException) {
            discard(item)
            throw e
        } catch (_: NotKept) {
            discard(item)
            return SaveResult.Failed("This phone could not keep $name.")
        } catch (e: ApiError) {
            discard(item)
            return SaveResult.Failed(if (e.fromServer) e.message ?: UNREACHABLE else UNREACHABLE)
        } catch (_: Exception) {
            discard(item)
            return SaveResult.Failed(UNREACHABLE)
        }
        return SaveResult.Saved(place.label)
    }

    private fun insert(name: String, mime: String, place: Place): Uri {
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, safeFileName(name))
            put(MediaStore.MediaColumns.MIME_TYPE, mime)
            put(MediaStore.MediaColumns.RELATIVE_PATH, place.relativePath)
            put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        return try {
            resolver.insert(place.collection(), values)
        } catch (e: IllegalArgumentException) { // a type this collection does not take
            throw NotKept(e)
        } ?: throw NotKept()
    }

    private fun discard(item: Uri?) {
        if (item == null) return
        try {
            resolver.delete(item, null, null)
        } catch (_: Exception) {
            // Still pending: MediaStore drops abandoned pending items itself after a while.
        }
    }
}
