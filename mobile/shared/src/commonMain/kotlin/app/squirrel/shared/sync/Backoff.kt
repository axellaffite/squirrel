package app.squirrel.shared.sync

import kotlin.math.roundToLong

private const val BASE_MS = 5_000L
private const val MAX_MS = 15 * 60_000L
private const val MAX_SHIFT = 20
private const val JITTER_FLOOR = 0.8
private const val JITTER_SPAN = 0.4

/** Delay before retry number [attempt] (0-based): 5 s · 2^n, at most 15 min, jitter 0.8–1.2. */
fun backoffMillis(attempt: Int, random: () -> Double): Long {
    val base = minOf(MAX_MS, BASE_MS shl attempt.coerceIn(0, MAX_SHIFT))
    return (base * (JITTER_FLOOR + JITTER_SPAN * random())).roundToLong()
}
