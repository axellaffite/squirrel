package app.squirrel.shared.sync

import kotlinx.coroutines.delay

/** How long a foreground run waits for the server to come back before it gives up. */
const val OFFLINE_GRACE_MS = 30 * 60_000L

/** The first wait between two reachability probes; it doubles up to [PROBE_MAX_MS]. */
const val PROBE_BASE_MS = 5_000L
const val PROBE_MAX_MS = 60_000L
private const val PROBE_MAX_SHIFT = 4

/** The wait before probe number [attempt] (0-based): 5 s, 10 s, 20 s, 40 s, then 60 s. */
fun probeDelay(attempt: Int): Long = minOf(PROBE_BASE_MS shl attempt.coerceIn(0, PROBE_MAX_SHIFT), PROBE_MAX_MS)

/**
 * Waits between two probes of the server while a run is offline. A platform may return early
 * when the network changes (Android: a ConnectivityManager callback), so the probe comes sooner.
 */
fun interface NetworkWaiter {
    suspend fun await(millis: Long)

    companion object {
        /** Just the time: no platform signal. */
        val Delay = NetworkWaiter { delay(it) }
    }
}
