package app.squirrel.shared.ios.transfer

import platform.Foundation.NSError
import platform.Foundation.NSHTTPURLResponse
import platform.Foundation.NSURLErrorCancelled
import platform.Foundation.NSURLErrorDomain
import platform.Foundation.NSURLSession
import platform.Foundation.NSURLSessionDelegateProtocol
import platform.Foundation.NSURLSessionTask
import platform.Foundation.NSURLSessionTaskDelegateProtocol
import platform.darwin.NSObject

/**
 * The session's delegate (a Kotlin class: Kotlin/Native implements the Objective-C protocols).
 * Called on the session's own queue, also when the app was relaunched in the background.
 */
class TransportDelegate(
    private val onComplete: (task: String, result: ChunkResult) -> Unit,
    private val onEventsFinished: () -> Unit,
) : NSObject(), NSURLSessionTaskDelegateProtocol, NSURLSessionDelegateProtocol {
    override fun URLSession(session: NSURLSession, task: NSURLSessionTask, didCompleteWithError: NSError?) {
        val description = task.taskDescription ?: return
        val status = (task.response as? NSHTTPURLResponse)?.statusCode?.toInt() ?: 0
        val cancelled = didCompleteWithError?.let { it.domain == NSURLErrorDomain && it.code == NSURLErrorCancelled } == true
        onComplete(description, ChunkResult(status, didCompleteWithError?.localizedDescription, cancelled))
    }

    override fun URLSessionDidFinishEventsForBackgroundURLSession(session: NSURLSession) = onEventsFinished()
}
