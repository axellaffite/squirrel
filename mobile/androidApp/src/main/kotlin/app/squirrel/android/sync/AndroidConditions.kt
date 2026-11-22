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

/** What backup needs to know of one network. */
data class NetworkFacts(val vpn: Boolean, val internet: Boolean, val unmetered: Boolean)

/**
 * Online and unmetered, from the default network and the others. Behind a VPN (Tailscale, used to
 * reach the server) the default network is the VPN, whose own flags may not tell Wi-Fi from mobile
 * data: then the networks under it decide, so "Only on Wi-Fi" neither pauses on Wi-Fi nor goes on
 * over mobile data.
 */
fun networkState(active: NetworkFacts?, others: List<NetworkFacts>): Pair<Boolean, Boolean> {
    if (active == null) return false to false
    if (!active.vpn) return active.internet to active.unmetered
    val underlying = others.filter { !it.vpn && it.internet }
    val online = active.internet || underlying.isNotEmpty()
    val unmetered = if (underlying.isEmpty()) active.unmetered else underlying.any { it.unmetered }
    return online to unmetered
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

    private fun facts(caps: NetworkCapabilities?): NetworkFacts? = caps?.let {
        NetworkFacts(
            vpn = it.hasTransport(NetworkCapabilities.TRANSPORT_VPN),
            internet = it.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET),
            unmetered = it.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED),
        )
    }

    private fun read(): DeviceState {
        val active = connectivity.activeNetwork
        val activeFacts = facts(connectivity.getNetworkCapabilities(active))
        // Only needed behind a VPN; allNetworks is the one way to see the networks under it.
        @Suppress("DEPRECATION")
        val others = if (activeFacts?.vpn == true) {
            connectivity.allNetworks.filter { it != active }.mapNotNull { facts(connectivity.getNetworkCapabilities(it)) }
        } else {
            emptyList()
        }
        val (online, unmetered) = networkState(activeFacts, others)
        val battery: Intent? = app.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        return DeviceState(
            online = online,
            unmetered = unmetered,
            charging = (battery?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0) != 0,
            batteryLow = battery?.getBooleanExtra(BatteryManager.EXTRA_BATTERY_LOW, false) == true,
        )
    }
}

private const val CACHE_MS = 5_000L
