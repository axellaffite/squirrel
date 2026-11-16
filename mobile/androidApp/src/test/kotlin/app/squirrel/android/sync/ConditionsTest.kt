package app.squirrel.android.sync

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.PauseReason
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
}
