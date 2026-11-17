package app.squirrel.shared.ios.photos

import app.squirrel.shared.sync.MediaAccess
import kotlinx.coroutines.suspendCancellableCoroutine
import platform.Photos.PHAccessLevelReadWrite
import platform.Photos.PHAsset
import platform.Photos.PHAuthorizationStatus
import platform.Photos.PHAuthorizationStatusAuthorized
import platform.Photos.PHAuthorizationStatusLimited
import platform.Photos.PHPhotoLibrary
import kotlin.coroutines.resume

/** The app's photo library access: Full, Limited ("Partial", with what is visible) or none. */
object PhotoAccess {
    fun status(): PHAuthorizationStatus = PHPhotoLibrary.authorizationStatusForAccessLevel(PHAccessLevelReadWrite)

    /** Whether the library may be touched without the system prompting (all or selected photos). */
    fun granted(): Boolean = isGranted(status())

    fun current(): MediaAccess = when (status()) {
        PHAuthorizationStatusAuthorized -> MediaAccess.Full
        PHAuthorizationStatusLimited -> MediaAccess.Partial(PHAsset.fetchAssetsWithOptions(null).count.toInt())
        else -> MediaAccess.None
    }

    /** The system prompt (once; afterwards only Settings can change it). */
    suspend fun request(): MediaAccess = suspendCancellableCoroutine { continuation ->
        PHPhotoLibrary.requestAuthorizationForAccessLevel(PHAccessLevelReadWrite) { _ ->
            continuation.resume(current())
        }
    }
}

/** Full or limited access: anything else, and PhotoKit calls would raise the system prompt. */
fun isGranted(status: PHAuthorizationStatus): Boolean =
    status == PHAuthorizationStatusAuthorized || status == PHAuthorizationStatusLimited
