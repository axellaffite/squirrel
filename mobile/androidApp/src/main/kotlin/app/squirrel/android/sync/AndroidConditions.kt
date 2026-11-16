package app.squirrel.android.sync

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.Conditions
import app.squirrel.shared.sync.PauseReason

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

/** Checked before each run and between chunks (WorkManager's constraints only gate the start). */
class AndroidConditions(context: Context) : Conditions {
    private val app = context.applicationContext
    private val connectivity = app.getSystemService(ConnectivityManager::class.java)

    override fun check(settings: BackupSettings): PauseReason? = pauseReason(settings, current())

    private fun current(): DeviceState {
        val caps = connectivity.getNetworkCapabilities(connectivity.activeNetwork)
        val battery: Intent? = app.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        return DeviceState(
            online = caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true,
            unmetered = caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) == true,
            charging = (battery?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0) != 0,
            batteryLow = battery?.getBooleanExtra(BatteryManager.EXTRA_BATTERY_LOW, false) == true,
        )
    }
}
