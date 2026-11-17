package app.squirrel.shared.sync

import app.squirrel.shared.settings.SettingsStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlin.math.roundToLong

/** Settings key: epoch ms of the last run that finished. */
const val LAST_RUN_KEY = "backup.last_run"

/** What the Backup screen shows, from the queue and the engine. */
data class BackupStatus(
    /** Assets selected for backup (skipped ones excluded). */
    val total: Long = 0,
    val backedUp: Long = 0,
    val left: Long = 0,
    val bytesLeft: Long = 0,
    val failed: Long = 0,
    val lastRun: Long? = null,
    val access: MediaAccess = MediaAccess.Full,
    val state: SyncState = SyncState.Idle,
) {
    val paused: PauseReason? get() = (state as? SyncState.Paused)?.reason

    /** The featured figure: "1 204 left", or that everything is backed up. */
    fun leftLabel(): String = when {
        total == 0L && left == 0L && lastRun == null -> "Ready to back up"
        left == 0L && failed == 0L -> "Everything is backed up"
        else -> "${formatCount(left)} left"
    }

    companion object {
        fun of(
            counts: Map<QueueState, Long>,
            bytesLeft: Long,
            state: SyncState,
            lastRun: Long?,
            access: MediaAccess,
        ): BackupStatus {
            fun n(vararg states: QueueState) = states.sumOf { counts[it] ?: 0L }
            return BackupStatus(
                total = counts.filterKeys { it != QueueState.Skipped }.values.sum(),
                backedUp = n(QueueState.Done),
                left = n(QueueState.New, QueueState.Hashed, QueueState.Missing, QueueState.Uploading),
                bytesLeft = bytesLeft,
                failed = n(QueueState.Failed),
                lastRun = lastRun,
                access = access,
                state = state,
            )
        }
    }
}

/** The live [BackupStatus]; [access] is re-read on every change (partial access can shrink). */
fun backupStatus(
    queue: Queue,
    engineState: StateFlow<SyncState>,
    settings: SettingsStore,
    access: () -> MediaAccess,
): Flow<BackupStatus> = combine(
    queue.countsFlow(),
    queue.bytesLeftFlow(),
    engineState,
    settings.valueFlow(LAST_RUN_KEY),
) { counts, bytes, state, lastRun ->
    BackupStatus.of(counts, bytes, state, lastRun?.toLongOrNull(), access())
}.distinctUntilChanged()

private const val GROUP_SEPARATOR = ' ' // narrow no-break space: "1 204"
private val UNITS = listOf("B", "KB", "MB", "GB", "TB")
private const val UNIT_STEP = 1000.0
private const val ONE_DECIMAL_BELOW = 100.0

/** A count with thousands grouped by a narrow no-break space (tabular in the UI). */
fun formatCount(n: Long): String {
    val digits = kotlin.math.abs(n).toString()
    val grouped = digits.reversed().chunked(3).joinToString(GROUP_SEPARATOR.toString()).reversed()
    return if (n < 0) "-$grouped" else grouped
}

/** Human-readable size in decimal units, e.g. `1.5 MB` (the web's formatBytes). */
fun formatBytes(bytes: Long): String {
    var value = bytes.toDouble()
    var unit = 0
    while (value >= UNIT_STEP && unit < UNITS.lastIndex) {
        value /= UNIT_STEP
        unit++
    }
    if (unit == 0 || value >= ONE_DECIMAL_BELOW) return "${value.roundToLong()} ${UNITS[unit]}"
    val tenths = (value * 10).roundToLong()
    return "${tenths / 10}.${tenths % 10} ${UNITS[unit]}"
}

private const val MINUTE = 60_000L
private const val HOUR = 60 * MINUTE
private const val DAY = 24 * HOUR

/** "Backed up 2 min ago", from the last finished run. */
fun formatLastRun(lastRun: Long?, now: Long): String {
    if (lastRun == null) return "Not backed up yet"
    val ago = (now - lastRun).coerceAtLeast(0)
    return "Backed up " + when {
        ago < MINUTE -> "just now"
        ago < HOUR -> "${ago / MINUTE} min ago"
        ago < DAY -> "${ago / HOUR} h ago"
        ago < 2 * DAY -> "yesterday"
        else -> "${ago / DAY} days ago"
    }
}
