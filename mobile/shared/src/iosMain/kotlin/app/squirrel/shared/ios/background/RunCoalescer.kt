package app.squirrel.shared.ios.background

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * One in-app run at a time. Requests that arrive during a run (a burst of library changes, a tap
 * on "Back up now") collapse into a single run after it, manual if any of them was. Confined to
 * the main thread, like its callers.
 */
class RunCoalescer(private val scope: CoroutineScope, private val start: suspend (manual: Boolean) -> Unit) {
    private var running = false
    private var pending: Boolean? = null

    fun request(manual: Boolean) {
        if (running) {
            pending = pending == true || manual
            return
        }
        running = true
        scope.launch {
            try {
                var next: Boolean? = manual
                while (next != null) {
                    try {
                        start(next)
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Exception) {
                        // A failed run (offline, server down) is the engine's to report; the next request runs again.
                        println("Squirrel: background run failed: ${e.message}")
                    }
                    next = pending
                    pending = null
                }
            } finally {
                running = false
                pending = null
            }
        }
    }
}
