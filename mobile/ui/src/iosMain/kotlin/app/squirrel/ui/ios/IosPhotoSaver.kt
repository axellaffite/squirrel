package app.squirrel.ui.ios

import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.files.safeFileName
import app.squirrel.ui.photos.PhotoSaver
import app.squirrel.ui.photos.SaveResult
import app.squirrel.ui.photos.UNREACHABLE
import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import platform.Foundation.NSFileManager
import platform.Foundation.NSURL
import platform.Photos.PHAccessLevelAddOnly
import platform.Photos.PHAssetCreationRequest
import platform.Photos.PHAssetResourceCreationOptions
import platform.Photos.PHAssetResourceTypePhoto
import platform.Photos.PHAssetResourceTypeVideo
import platform.Photos.PHAuthorizationStatus
import platform.Photos.PHAuthorizationStatusAuthorized
import platform.Photos.PHAuthorizationStatusLimited
import platform.Photos.PHAuthorizationStatusNotDetermined
import platform.Photos.PHPhotoLibrary
import platform.UIKit.UIActivityViewController
import platform.UIKit.UIViewController
import platform.UIKit.popoverPresentationController
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

private const val NO_ACCESS = "Squirrel may not add to your photo library: allow it in Settings › Squirrel › Photos."
private const val NO_SHEET = "The share sheet could not open. Try again."

/**
 * Saves downloads on iOS: an original is downloaded into `Caches/open/save-<id>/` and moved into the
 * Photos library (`PHAssetCreationRequest`, add-only access); a zip is downloaded into
 * `Caches/open/zip/` and handed to the share sheet, which offers Save to Files. `Caches/open` is
 * [IosFileOpener]'s folder: cleared at launch and at sign-out.
 */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class IosPhotoSaver(
    private val dir: String = "${cachesPath()}/open",
    /** Where the share sheet is presented from; null when the app has no window. */
    private val host: () -> UIViewController? = ::topViewController,
) : PhotoSaver {
    private val files = NSFileManager.defaultManager

    override suspend fun saveOriginal(api: SquirrelApi, id: String, name: String, mime: String?, onProgress: (Float) -> Unit): SaveResult {
        if (!addAccess()) return SaveResult.Failed(NO_ACCESS)
        val folder = "$dir/save-${id.filter { it.isLetterOrDigit() || it == '-' }}"
        val target = "$folder/${safeFileName(name)}"
        fetch(api, api.contentUrl(id), folder, target, onProgress)?.let { return it }
        return try {
            addToLibrary(target, name, video = mime?.startsWith("video/") == true)
            SaveResult.Saved("Photos")
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            SaveResult.Failed("Photos could not keep $name.")
        } finally {
            files.removeItemAtPath(folder, null)
        }
    }

    override suspend fun saveArchive(api: SquirrelApi, url: String, name: String, onProgress: (Float) -> Unit): SaveResult {
        val folder = "$dir/zip"
        val target = "$folder/${safeFileName(name)}"
        fetch(api, url, folder, target, onProgress)?.let { return it }
        val host = host() ?: return SaveResult.Failed(NO_SHEET)
        val sheet = UIActivityViewController(listOf(NSURL.fileURLWithPath(target)), null)
        sheet.popoverPresentationController?.sourceView = host.view // iPad: anchored to the app's view
        host.presentViewController(sheet, animated = true, completion = null)
        return SaveResult.Handed
    }

    /** Downloads [url] to [target] (through a `.part` file); null when it is there, else why not. */
    private suspend fun fetch(api: SquirrelApi, url: String, folder: String, target: String, onProgress: (Float) -> Unit): SaveResult.Failed? {
        val partial = "$target.part"
        try {
            withContext(Dispatchers.IO) {
                files.createDirectoryAtPath(folder, withIntermediateDirectories = true, attributes = IosFileOpener.PROTECTED, error = null)
                downloadToFile(api, url, partial, IosFileOpener.PROTECTED, onProgress)
                files.removeItemAtPath(target, null) // the same file downloaded earlier
                if (!files.moveItemAtPath(partial, toPath = target, error = null)) error("could not keep $target")
            }
        } catch (e: CancellationException) {
            files.removeItemAtPath(partial, null)
            throw e
        } catch (e: ApiError) {
            files.removeItemAtPath(partial, null)
            return SaveResult.Failed(if (e.fromServer) e.message ?: UNREACHABLE else UNREACHABLE)
        } catch (_: Exception) {
            files.removeItemAtPath(partial, null)
            return SaveResult.Failed(UNREACHABLE)
        }
        return null
    }

    /** Add-only access to the library (asks the first time); full access, granted for backups, counts too. */
    private suspend fun addAccess(): Boolean {
        val status = PHPhotoLibrary.authorizationStatusForAccessLevel(PHAccessLevelAddOnly)
        if (status != PHAuthorizationStatusNotDetermined) return canAdd(status)
        return suspendCancellableCoroutine { continuation ->
            PHPhotoLibrary.requestAuthorizationForAccessLevel(PHAccessLevelAddOnly) { granted -> continuation.resume(canAdd(granted)) }
        }
    }

    private fun canAdd(status: PHAuthorizationStatus) = status == PHAuthorizationStatusAuthorized || status == PHAuthorizationStatusLimited

    /** Moves the file at [path] into the library as a new photo or video named [name]. */
    private suspend fun addToLibrary(path: String, name: String, video: Boolean) = suspendCancellableCoroutine { continuation ->
        PHPhotoLibrary.sharedPhotoLibrary().performChanges({
            val options = PHAssetResourceCreationOptions().apply {
                originalFilename = name
                shouldMoveFile = true
            }
            PHAssetCreationRequest.creationRequestForAsset().addResourceWithType(
                if (video) PHAssetResourceTypeVideo else PHAssetResourceTypePhoto,
                fileURL = NSURL.fileURLWithPath(path),
                options = options,
            )
        }) { success, error ->
            if (success) continuation.resume(Unit) else continuation.resumeWithException(IllegalStateException(error?.localizedDescription ?: "not saved"))
        }
    }
}
