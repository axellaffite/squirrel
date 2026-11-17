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

/** The phone's network and power, as backup sees them. */
data class DeviceState(val online: Boolean, val unmetered: Boolean, val charging: Boolean, val batteryLow: Boolean)

/** Pure: why a run must wait under [settings] in [state], or null. */
fun pauseReason(settings: BackupSettings, state: DeviceState): PauseReason? = when {
    !state.online -> PauseReason.NoNetwork
    settings.wifiOnly && !state.unmetered -> PauseReason.WifiRequired
    settings.chargingOnly && !state.charging -> PauseReason.ChargingRequired
    state.batteryLow && !state.charging -> PauseReason.BatteryLow
    else -> null
}
