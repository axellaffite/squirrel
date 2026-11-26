package app.squirrel.android.files

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.HandoffResult
import app.squirrel.ui.files.streamDownload
import app.squirrel.ui.photos.UNREACHABLE
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Downloads a file's original into `cacheDir/open/` (private, cleared on sign-out) and hands it to
 * another app through a FileProvider URI with a one-off read grant.
 */
class AndroidFileOpener(context: Context) : FileHandoff {
    private val app = context.applicationContext
    private val cache = OpenCache(File(app.cacheDir, "open"))

    /** At app start: drop what earlier sessions left behind. */
    fun prune() = cache.prune()

    override suspend fun open(
        api: SquirrelApi,
        id: String,
        name: String,
        mime: String?,
        share: Boolean,
        onProgress: (Float) -> Unit,
    ): HandoffResult {
        val target = cache.target(id, name)
        val temp = cache.temp(target)
        try {
            withContext(Dispatchers.IO) {
                cache.prune(keepId = id)
                download(api, id, temp, onProgress)
                cache.commit(temp, target)
            }
        } catch (e: CancellationException) {
            temp.delete()
            throw e
        } catch (e: ApiError) {
            temp.delete()
            return HandoffResult.Failed(e.message ?: UNREACHABLE)
        } catch (_: Exception) {
            temp.delete()
            return HandoffResult.Failed(UNREACHABLE)
        }
        val uri = FileProvider.getUriForFile(app, "${app.packageName}.files", target)
        val type = mime ?: "application/octet-stream"
        val intent = if (share) {
            Intent.createChooser(Intent(Intent.ACTION_SEND).setType(type).putExtra(Intent.EXTRA_STREAM, uri), null)
        } else {
            Intent(Intent.ACTION_VIEW).setDataAndType(uri, type)
        }.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        return try {
            app.startActivity(intent)
            HandoffResult.Opened
        } catch (_: ActivityNotFoundException) {
            HandoffResult.NoApp
        }
    }

    private suspend fun download(api: SquirrelApi, id: String, target: File, onProgress: (Float) -> Unit) {
        target.parentFile?.mkdirs()
        target.outputStream().use { out ->
            streamDownload(api, api.contentUrl(id), onProgress) { buffer, n -> out.write(buffer, 0, n) }
        }
    }

    override fun clear() = cache.clear()
}
