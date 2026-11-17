package app.squirrel.shared.ios.photos

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import platform.Photos.PHChange
import platform.Photos.PHPhotoLibrary
import platform.Photos.PHPhotoLibraryChangeObserverProtocol
import platform.darwin.NSObject

/** How long the library must stay quiet before a change triggers a run (an import is many changes). */
const val LIBRARY_DEBOUNCE_MS = 5_000L

/** Photos added or edited while the app is active: [onChange] once things settle. */
class LibraryObserver(private val scope: CoroutineScope, private val onChange: () -> Unit) :
    NSObject(), PHPhotoLibraryChangeObserverProtocol {
    private var pending: Job? = null
    private var registered = false

    override fun photoLibraryDidChange(changeInstance: PHChange) {
        scope.launch {
            pending?.cancel()
            pending = scope.launch {
                delay(LIBRARY_DEBOUNCE_MS)
                onChange()
            }
        }
    }

    fun register() {
        if (registered) return
        PHPhotoLibrary.sharedPhotoLibrary().registerChangeObserver(this)
        registered = true
    }

    fun unregister() {
        if (!registered) return
        PHPhotoLibrary.sharedPhotoLibrary().unregisterChangeObserver(this)
        registered = false
        pending?.cancel()
    }
}
