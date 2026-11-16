package app.squirrel.shared.sync

import app.squirrel.shared.settings.BackupSettings

/** Why a run waits. */
enum class PauseReason {
    WifiRequired,
    ChargingRequired,
    BatteryLow,
    NoNetwork,
    NoPhotoAccess,

    /** The user's quota or the server's disk is full ("Storage full"). */
    StorageFull,
}

/** The phone's state as backup cares about it (network, power); platform-specific. */
interface Conditions {
    /** Null when a run may go on under [settings], else why it must wait. */
    fun check(settings: BackupSettings): PauseReason?
}
