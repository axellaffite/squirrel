package app.squirrel.android.sync

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.Conditions
import app.squirrel.shared.sync.DeviceState
import app.squirrel.shared.sync.PauseReason
import app.squirrel.shared.sync.pauseReason

/** Remembers a value for [ttlMillis]; the clock is injectable for tests. */
class Cached<T>(private val ttlMillis: Long, private val now: () -> Long, private val load: () -> T) {
    private var value: T? = null
    private var at = 0L

    @Synchronized
    fun get(): T {
        val time = now()
        val cached = value
        if (cached != null && time - at < ttlMillis) return cached
        return load().also {
            value = it
            at = time
        }
    }
}

/**
 * Checked before each run and between chunks (WorkManager's constraints only gate the start).
 * The device state is read at most every 5 s: chunks check it often.
 */
class AndroidConditions(context: Context) : Conditions {
    private val app = context.applicationContext
    private val connectivity = app.getSystemService(ConnectivityManager::class.java)
    private val state = Cached(CACHE_MS, System::currentTimeMillis, ::read)

    override fun check(settings: BackupSettings): PauseReason? = pauseReason(settings, state.get())

    private fun read(): DeviceState {
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

private const val CACHE_MS = 5_000L
