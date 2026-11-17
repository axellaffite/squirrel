package app.squirrel.ui.ios

/** `application(_:didFinishLaunchingWithOptions:)`: background task handlers register here. */
fun onLaunch() {
    IosGraph.instance
}

/** `application(_:handleEventsForBackgroundURLSession:completionHandler:)`. */
fun onBackgroundUrlSessionEvents(identifier: String, completionHandler: () -> Unit) {
    completionHandler()
}
