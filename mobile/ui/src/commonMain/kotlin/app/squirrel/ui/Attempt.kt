package app.squirrel.ui

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive

/**
 * Runs a network call for a screen: null on any failure. A CancellationException is rethrown
 * only when this coroutine really is cancelled; otherwise (a timeout subclass, a library that
 * cancels internally) it is a failure like any other, so the screen shows "Try again".
 */
suspend fun <T> attempt(call: suspend () -> T): T? = try {
    call()
} catch (e: CancellationException) {
    if (!currentCoroutineContext().isActive) throw e
    null
} catch (_: Exception) {
    null
}
