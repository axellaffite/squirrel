package app.squirrel.ui.ios

import app.squirrel.shared.ios.photos.PhotoAccess
import app.squirrel.ui.backup.PhotoAccessActions
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.launch
import platform.Foundation.NSURL
import platform.Photos.PHPhotoLibrary
import platform.PhotosUI.presentLimitedLibraryPickerFromViewController
import platform.UIKit.UIApplication
import platform.UIKit.UIApplicationOpenSettingsURLString

/** iOS photo access: the system prompt, the Limited Access picker, the app's Settings page. */
object IosPhotoActions : PhotoAccessActions {
    private val scope = MainScope()

    override fun requestAccess() {
        scope.launch {
            PhotoAccess.request()
            // Granted (fully or in part): rescan for what is now visible.
            IosGraph.instance.onAppVisible()
            IosGraph.instance.backUpNow()
        }
    }

    override fun changeSelection() {
        val host = topViewController() ?: return
        PHPhotoLibrary.sharedPhotoLibrary().presentLimitedLibraryPickerFromViewController(host)
    }

    override fun allowAll() {
        val url = NSURL.URLWithString(UIApplicationOpenSettingsURLString) ?: return
        UIApplication.sharedApplication.openURL(url, options = emptyMap<Any?, Any>(), completionHandler = null)
    }
}
