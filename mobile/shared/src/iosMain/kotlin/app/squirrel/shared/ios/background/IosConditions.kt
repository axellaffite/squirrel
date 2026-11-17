package app.squirrel.shared.ios.background

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.Conditions
import app.squirrel.shared.sync.DeviceState
import app.squirrel.shared.sync.PauseReason
import app.squirrel.shared.sync.pauseReason
import kotlin.concurrent.AtomicReference
import platform.Foundation.NSNotificationCenter
import platform.Foundation.NSOperationQueue
import platform.Foundation.NSProcessInfo
import platform.Foundation.NSProcessInfoPowerStateDidChangeNotification
import platform.Foundation.isLowPowerModeEnabled
import platform.Network.nw_path_get_status
import platform.Network.nw_path_is_constrained
import platform.Network.nw_path_is_expensive
import platform.Network.nw_path_monitor_create
import platform.Network.nw_path_monitor_set_queue
import platform.Network.nw_path_monitor_set_update_handler
import platform.Network.nw_path_monitor_start
import platform.Network.nw_path_status_satisfied
import platform.UIKit.UIDevice
import platform.UIKit.UIDeviceBatteryState
import platform.UIKit.UIDeviceBatteryStateDidChangeNotification
import platform.darwin.DISPATCH_QUEUE_PRIORITY_DEFAULT
import platform.darwin.dispatch_get_global_queue

/**
 * The phone's network ([nw_path_monitor]: "expensive" is cellular or a hotspot, "constrained" is
 * Low Data Mode) and power (charging, Low Power Mode), kept current by system callbacks so that
 * [check] is cheap between chunks. Create it on the main thread (UIDevice).
 */
class IosConditions : Conditions {
    private val state = AtomicReference(DeviceState(online = true, unmetered = true, charging = false, batteryLow = false))
    private val monitor = nw_path_monitor_create()

    init {
        UIDevice.currentDevice.batteryMonitoringEnabled = true
        updatePower()
        val center = NSNotificationCenter.defaultCenter
        center.addObserverForName(UIDeviceBatteryStateDidChangeNotification, null, NSOperationQueue.mainQueue) { _ -> updatePower() }
        center.addObserverForName(NSProcessInfoPowerStateDidChangeNotification, null, NSOperationQueue.mainQueue) { _ -> updatePower() }
        nw_path_monitor_set_update_handler(monitor) { path ->
            val online = nw_path_get_status(path) == nw_path_status_satisfied
            val unmetered = online && !nw_path_is_expensive(path) && !nw_path_is_constrained(path)
            update { it.copy(online = online, unmetered = unmetered) }
        }
        nw_path_monitor_set_queue(monitor, dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT.toLong(), 0u))
        nw_path_monitor_start(monitor)
    }

    override fun check(settings: BackupSettings): PauseReason? = pauseReason(settings, state.value)

    /** Whether iCloud originals may be downloaded now under "Only on Wi-Fi". */
    fun unmetered(): Boolean = state.value.unmetered

    private fun updatePower() {
        val battery = UIDevice.currentDevice.batteryState
        val charging = battery == UIDeviceBatteryState.UIDeviceBatteryStateCharging || battery == UIDeviceBatteryState.UIDeviceBatteryStateFull
        // Low Power Mode is iOS's "battery saver": the user asked the phone to do less.
        val low = NSProcessInfo.processInfo.isLowPowerModeEnabled()
        update { it.copy(charging = charging, batteryLow = low) }
    }

    private fun update(change: (DeviceState) -> DeviceState) {
        while (true) {
            val current = state.value
            if (state.compareAndSet(current, change(current))) return
        }
    }
}
