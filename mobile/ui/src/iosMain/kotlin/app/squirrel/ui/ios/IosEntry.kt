package app.squirrel.ui.ios

import app.squirrel.shared.ios.photos.LibraryObserver
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.launch
import platform.Foundation.NSNotificationCenter
import platform.Foundation.NSOperationQueue
import platform.UIKit.UIApplicationDidBecomeActiveNotification
import platform.UIKit.UIApplicationWillResignActiveNotification
import platform.darwin.dispatch_async
import platform.darwin.dispatch_get_main_queue

/** A background relaunch gets about 30 s: the engine stops a little before. */
private const val RELAUNCH_BUDGET_MS = 20_000L

private val entryScope = MainScope()

private val observer by lazy {
    LibraryObserver(entryScope) { IosGraph.backup.runSoon(IosGraph.instance.settings.backup()) }
}

/**
 * `application(_:didFinishLaunchingWithOptions:)`: background task handlers register here (they
 * must be, before launch finishes), and the app follows its own activity.
 */
fun onLaunch() {
    val backup = IosGraph.backup
    backup.tasks.register()
    IosGraph.instance
    val center = NSNotificationCenter.defaultCenter
    center.addObserverForName(UIApplicationDidBecomeActiveNotification, null, NSOperationQueue.mainQueue) { _ ->
        IosGraph.instance.onAppVisible()
        observer.register()
    }
    center.addObserverForName(UIApplicationWillResignActiveNotification, null, NSOperationQueue.mainQueue) { _ ->
        observer.unregister()
    }
}

/**
 * `application(_:handleEventsForBackgroundURLSession:completionHandler:)`: the system relaunched the
 * app for finished chunk transfers. The delegate records them; then the engine completes what it
 * can and queues the next chunks, and the system gets its completion handler back.
 */
fun onBackgroundUrlSessionEvents(identifier: String, completionHandler: () -> Unit) {
    val backup = IosGraph.backup
    backup.transfers.onEventsFinished = {
        entryScope.launch {
            try {
                GraphRunner { IosGraph.instance }.run(
                    deadline = kotlin.time.Clock.System.now().toEpochMilliseconds() + RELAUNCH_BUDGET_MS,
                    manual = false,
                )
            } finally {
                dispatch_async(dispatch_get_main_queue()) { completionHandler() }
            }
        }
    }
}
