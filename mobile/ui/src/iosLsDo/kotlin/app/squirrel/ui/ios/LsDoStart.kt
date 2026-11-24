package app.squirrel.ui.ios

import app.squirrel.lsdo.LsDo
import app.squirrel.lsdo.runtime.LSDO_PORT
import app.squirrel.lsdo.runtime.LsDoRegistry
import app.squirrel.lsdo.runtime.startLsDoServer
import kotlinx.coroutines.Dispatchers
import platform.Foundation.NSLog

/**
 * Debug builds (Xcode's Debug configuration): screens register into an LS-DO registry served on
 * the device's loopback. The simulator shares the Mac's: `LSDO_URL=http://127.0.0.1:8765 scripts/lsdo ls`.
 */
internal fun startLsDo() {
    val registry = LsDoRegistry(context = Dispatchers.Main.immediate, capture = UiKitCapture::capture)
    LsDo.install(registry)
    runCatching { startLsDoServer(registry) }
        .onSuccess { NSLog("LS-DO listening on 127.0.0.1:$LSDO_PORT") }
        .onFailure { NSLog("LS-DO server did not start: %@", it.toString()) }
}
