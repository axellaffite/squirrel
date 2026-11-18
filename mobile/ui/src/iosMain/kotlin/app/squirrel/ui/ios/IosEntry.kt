package app.squirrel.ui.ios

import app.squirrel.shared.ios.photos.LibraryObserver
import app.squirrel.shared.ios.transfer.SessionCompletion
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
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
    val graph = IosGraph.instance
    // Files handed to other apps in an earlier process: no sheet can still be reading them.
    entryScope.launch(Dispatchers.IO) { graph.files?.clear() }
    val center = NSNotificationCenter.defaultCenter
    center.addObserverForName(UIApplicationDidBecomeActiveNotification, null, NSOperationQueue.mainQueue) { _ ->
        IosGraph.instance.onAppVisible()
        observer.register()
    }
    center.addObserverForName(UIApplicationWillResignActiveNotification, null, NSOperationQueue.mainQueue) { _ ->
        observer.unregister()
    }
}

/** The system's handler for a background relaunch, called once the engine is done. */
private val sessionCompletion = SessionCompletion()

/**
 * `application(_:handleEventsForBackgroundURLSession:completionHandler:)`: the system relaunched the
 * app for finished chunk transfers. The handler is stored first, then the session is recreated: its
 * delegate records the results and calls [backgroundEventsFinished].
 */
fun onBackgroundUrlSessionEvents(identifier: String, completionHandler: () -> Unit) {
    sessionCompletion.onEvents(
        identifier,
        handler = { dispatch_async(dispatch_get_main_queue()) { completionHandler() } },
        createSession = { IosGraph.backup.transfers },
    )
}

/**
 * Every event was delivered: the engine completes what it can and queues the next chunks, then the
 * system gets its handler back. Nothing to do when the app was not relaunched for these events.
 */
internal fun backgroundEventsFinished() {
    if (!sessionCompletion.isPending) return
    entryScope.launch {
        try {
            GraphRunner { IosGraph.instance }.run(
                deadline = kotlin.time.Clock.System.now().toEpochMilliseconds() + RELAUNCH_BUDGET_MS,
                manual = false,
            )
        } finally {
            sessionCompletion.finish()
        }
    }
}
