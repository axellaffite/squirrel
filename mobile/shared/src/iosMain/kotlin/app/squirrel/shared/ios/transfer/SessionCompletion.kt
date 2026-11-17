package app.squirrel.shared.ios.transfer

import kotlin.concurrent.AtomicReference

/**
 * The completion handler of `application(_:handleEventsForBackgroundURLSession:completionHandler:)`.
 * It is stored before the session is created (creating it is what delivers the events), and called
 * exactly once, by whichever [finish] comes first.
 */
class SessionCompletion {
    private val pending = AtomicReference<(() -> Unit)?>(null)

    /** Whether the system waits for a handler: the app was relaunched for this session's events. */
    val isPending: Boolean get() = pending.value != null

    /**
     * Routes the system's call: a session other than [UPLOAD_SESSION_ID] is not ours, so its
     * handler runs at once. Ours is stored, then [createSession] recreates the session.
     */
    fun onEvents(identifier: String, handler: () -> Unit, createSession: () -> Unit) {
        if (identifier != UPLOAD_SESSION_ID) {
            handler()
            return
        }
        pending.value = handler
        createSession()
    }

    /** Hands the system its handler back, once; later calls do nothing. */
    fun finish() {
        pending.getAndSet(null)?.invoke()
    }
}
