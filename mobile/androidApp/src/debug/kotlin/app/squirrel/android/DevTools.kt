package app.squirrel.android

import app.squirrel.ui.LsDoScreens
import io.github.axellaffite.lsdo.runtime.compose.LsDoDevTools

/**
 * Debug builds: screens register into an LS-DO registry served on the phone's loopback (port
 * 8765); `scripts/lsdo` reaches it through `adb forward`. Release has an empty twin.
 */
internal object DevTools {
    fun start() {
        LsDoDevTools.start(catalog = LsDoScreens)
    }
}
