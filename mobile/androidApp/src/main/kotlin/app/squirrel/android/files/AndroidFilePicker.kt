package app.squirrel.android.files

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import app.squirrel.android.media.MediaStoreReader
import app.squirrel.shared.sync.AssetReader
import app.squirrel.ui.files.FilePicker
import app.squirrel.ui.files.PickedFile
import app.squirrel.ui.files.safeFileName
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.FileNotFoundException

/**
 * The system's document picker (it also offers Photos and other apps' files), registered by the
 * activity before it starts. Picked files are read through their content URI, never copied.
 */
class AndroidFilePicker(activity: ComponentActivity) : FilePicker {
    private val resolver = activity.contentResolver
    private var pending: CompletableDeferred<List<Uri>>? = null
    private val launcher = activity.registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        pending?.complete(uris)
        pending = null
    }

    override suspend fun pick(): List<PickedFile> {
        pending?.complete(emptyList())
        val result = CompletableDeferred<List<Uri>>()
        pending = result
        launcher.launch(arrayOf("*/*"))
        return result.await().map { uri -> PickedFile(nameOf(resolver, uri), open = { open(uri) }) }
    }

    private suspend fun open(uri: Uri): AssetReader? = withContext(Dispatchers.IO) {
        try {
            val descriptor = resolver.openFileDescriptor(uri, "r") ?: return@withContext null
            MediaStoreReader(descriptor, descriptor.statSize, modifiedAt = null)
        } catch (_: FileNotFoundException) {
            null
        } catch (_: SecurityException) {
            null
        }
    }
}

/** The name the providing app gives the file, made safe as one path segment. */
private fun nameOf(resolver: ContentResolver, uri: Uri): String {
    val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) cursor.getString(0) else null
    }
    return safeFileName(name ?: uri.lastPathSegment ?: "file")
}
