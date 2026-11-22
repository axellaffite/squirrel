package app.squirrel.ui.backup

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.BackupStatus
import app.squirrel.shared.sync.ErrorReason
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.shared.sync.PauseReason
import app.squirrel.shared.sync.SyncState
import app.squirrel.shared.sync.formatBytes
import app.squirrel.shared.sync.formatCount
import app.squirrel.shared.sync.formatLastRun

/** How much of the library the screen can promise. */
sealed interface AccessUi {
    data object Full : AccessUi
    data class Partial(val text: String) : AccessUi
    /** Videos are to be backed up but cannot be read. */
    data class NoVideos(val text: String) : AccessUi
    data object None : AccessUi
}

/** Everything the Backup screen shows, as text (MASTER.md: real figures only). */
data class BackupUi(
    val featured: String,
    val detail: String,
    val lastRun: String,
    val progress: Float?,
    val progressLabel: String?,
    val running: Boolean,
    val button: String,
    val backedUp: String,
    val waiting: String,
    val failed: String,
    val failedCount: Long,
    val notice: String?,
    val noticeIsError: Boolean,
    val access: AccessUi,
    val locationWarning: String?,
)

/** Pure mapping, tested string by string. */
fun backupUi(status: BackupStatus, settings: BackupSettings, locationsIncluded: Boolean, now: Long): BackupUi {
    val state = status.state
    val running = state is SyncState.Scanning || state is SyncState.Hashing || state is SyncState.Checking ||
        state is SyncState.Uploading || state is SyncState.WaitingForNetwork
    val (progress, progressLabel) = when (state) {
        is SyncState.Uploading ->
            (if (state.of <= 0) 0f else ((state.done + state.current) / state.of).coerceIn(0f, 1f)) to "Uploading ${state.name}"
        is SyncState.Hashing -> fraction(state.done, state.of) to "Preparing ${formatCount(state.of.toLong())} items"
        SyncState.Scanning -> null to "Looking for new photos"
        SyncState.Checking -> null to "Checking what your Squirrel already has"
        // The run goes on by itself once the server answers again (a VPN reconnecting, say).
        SyncState.WaitingForNetwork -> null to "Waiting for the network to reach your Squirrel"
        else -> null to null
    }
    val (notice, isError) = when (state) {
        is SyncState.Paused -> pauseText(state.reason) to (state.reason == PauseReason.StorageFull)
        is SyncState.Error -> errorText(state.reason) to true
        else -> null to false
    }
    return BackupUi(
        featured = status.leftLabel(),
        detail = "${formatBytes(status.bytesLeft)} · ${if (settings.wifiOnly) "Wi-Fi only" else "Any network"}",
        lastRun = formatLastRun(status.lastRun, now),
        progress = progress,
        progressLabel = progressLabel,
        running = running,
        button = if (running) "Backing up…" else "Back up now",
        backedUp = formatCount(status.backedUp),
        waiting = formatCount(status.left),
        failed = formatCount(status.failed),
        failedCount = status.failed,
        notice = notice,
        noticeIsError = isError,
        access = when (val access = status.access) {
            MediaAccess.Full -> AccessUi.Full
            is MediaAccess.Partial -> AccessUi.Partial(
                "Squirrel can see ${formatCount(access.count.toLong())} selected " +
                    "${if (access.count == 1) "photo" else "photos"}. Only these are backed up.",
            )
            MediaAccess.None -> AccessUi.None
            MediaAccess.NoVideos ->
                if (settings.includeVideos) AccessUi.NoVideos("Squirrel cannot see your videos. Only photos are backed up.")
                else AccessUi.Full
        },
        locationWarning = if (status.access != MediaAccess.None && !locationsIncluded) "Photo locations won't be backed up" else null,
    )
}

private fun fraction(done: Int, of: Int): Float = if (of <= 0) 0f else done.toFloat() / of

fun pauseText(reason: PauseReason): String = when (reason) {
    PauseReason.WifiRequired -> "Waiting for Wi-Fi"
    PauseReason.ChargingRequired -> "Waiting for charging"
    PauseReason.BatteryLow -> "Waiting for the battery to charge"
    PauseReason.NoNetwork -> "Waiting for a network connection"
    PauseReason.NoPhotoAccess -> "Squirrel cannot see your photos"
    PauseReason.StorageFull -> "Storage full: free some space on your Squirrel to continue"
}

fun errorText(reason: ErrorReason): String = when (reason) {
    ErrorReason.Unreachable -> "Your Squirrel could not be reached. Backup will try again."
    ErrorReason.SignedOut -> "This phone was signed out"
    ErrorReason.Unexpected -> "Backup stopped because of an error. It will try again."
}
