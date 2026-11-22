package app.squirrel.android.sync

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.DeviceState
import app.squirrel.shared.sync.PauseReason
import app.squirrel.shared.sync.pauseReason
import org.junit.Test
import kotlin.test.assertEquals

class ConditionsTest {
    private val ok = DeviceState(online = true, unmetered = true, charging = false, batteryLow = false)

    @Test
    fun pauseReasonsFollowTheSettings() {
        assertEquals(null, pauseReason(BackupSettings(), ok))
        assertEquals(PauseReason.NoNetwork, pauseReason(BackupSettings(), ok.copy(online = false)))
        assertEquals(PauseReason.WifiRequired, pauseReason(BackupSettings(wifiOnly = true), ok.copy(unmetered = false)))
        assertEquals(null, pauseReason(BackupSettings(wifiOnly = false), ok.copy(unmetered = false)))
        assertEquals(PauseReason.ChargingRequired, pauseReason(BackupSettings(chargingOnly = true), ok))
        assertEquals(PauseReason.BatteryLow, pauseReason(BackupSettings(), ok.copy(batteryLow = true)))
        assertEquals(null, pauseReason(BackupSettings(), ok.copy(batteryLow = true, charging = true)))
    }

    private val wifi = NetworkFacts(vpn = false, internet = true, unmetered = true)
    private val mobile = NetworkFacts(vpn = false, internet = true, unmetered = false)

    @Test
    fun withoutAVpnTheDefaultNetworkDecides() {
        assertEquals(true to true, networkState(wifi, emptyList()))
        assertEquals(true to false, networkState(mobile, emptyList()))
        assertEquals(false to false, networkState(null, emptyList()))
    }

    @Test
    fun behindAVpnTheNetworksUnderItDecide() {
        // Tailscale's network may say "metered" (or nothing) whatever carries it.
        val tailscale = NetworkFacts(vpn = true, internet = true, unmetered = false)
        assertEquals(true to true, networkState(tailscale, listOf(wifi, tailscale))) // on Wi-Fi: not paused
        assertEquals(true to false, networkState(tailscale, listOf(mobile))) // on mobile data: waits for Wi-Fi
        assertEquals(true to true, networkState(tailscale, listOf(wifi, mobile))) // Wi-Fi with mobile data on standby
        // A VPN whose own flags are all there is.
        assertEquals(true to true, networkState(tailscale.copy(unmetered = true), emptyList()))
        assertEquals(false to false, networkState(tailscale.copy(internet = false), emptyList()))
    }

    @Test
    fun theDeviceStateIsReadAtMostEveryFiveSeconds() {
        var clock = 0L
        var reads = 0
        val cached = Cached(5_000, { clock }) { ++reads }
        assertEquals(1, cached.get())
        clock = 4_999
        assertEquals(1, cached.get())
        clock = 5_000
        assertEquals(2, cached.get())
    }
}
