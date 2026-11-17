package app.squirrel.ui.ios

import platform.UIKit.UIApplication
import platform.UIKit.UIViewController
import platform.UIKit.UIWindow
import platform.UIKit.UIWindowScene

/** The view controller on top of the app's key window: where system sheets are presented. */
fun topViewController(): UIViewController? {
    val window = UIApplication.sharedApplication.connectedScenes
        .mapNotNull { it as? UIWindowScene }
        .flatMap { scene -> scene.windows.mapNotNull { it as? UIWindow } }
        .firstOrNull { it.isKeyWindow() }
    var top = window?.rootViewController
    while (top?.presentedViewController != null) top = top.presentedViewController
    return top
}
