package app.squirrel.shared.sync

enum class ErrorReason { SignedOut, Unreachable, Unexpected }

sealed interface SyncState {
    data object Idle : SyncState
    data object Scanning : SyncState
    data class Hashing(val done: Int, val of: Int) : SyncState
    data object Checking : SyncState
    data class Uploading(val name: String, val done: Int, val of: Int) : SyncState
    data class Paused(val reason: PauseReason) : SyncState
    data class Error(val reason: ErrorReason) : SyncState
}

enum class StopCause { Finished, Paused, Deadline, Error, AlreadyRunning }

data class RunSummary(
    val hashed: Int = 0,
    val checked: Int = 0,
    val uploaded: Int = 0,
    val deduplicated: Int = 0,
    val failed: Int = 0,
    val stoppedBy: StopCause = StopCause.Finished,
)
