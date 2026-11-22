package app.squirrel.android.sync

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import app.squirrel.shared.sync.NetworkWaiter
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Wakes an offline run's wait when the default network changes (a VPN reconnecting, Wi-Fi coming
 * back), so the next probe of the server comes at once instead of after the full delay.
 * Registered once for the process; the first callbacks (the network at registration) are not changes.
 */
class NetworkChanges(context: Context) : NetworkWaiter {
    private val version = MutableStateFlow(0)
    private val validated = mutableMapOf<Network, Boolean>()

    init {
        val connectivity = context.applicationContext.getSystemService(ConnectivityManager::class.java)
        connectivity.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) = changed()

            override fun onLost(network: Network) {
                synchronized(validated) { validated.remove(network) }
                changed()
            }

            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                // Signal strength and the like change often; only "usable now" matters.
                val now = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
                val before = synchronized(validated) { validated.put(network, now) }
                if (before != null && before != now) changed()
            }
        })
    }

    private fun changed() = version.update { it + 1 }

    override suspend fun await(millis: Long) {
        val seen = version.value
        withTimeoutOrNull(millis) { version.first { it != seen } }
    }
}
