package app.squirrel.ui.ios

import app.squirrel.ui.LsDoScreens
import io.github.axellaffite.lsdo.runtime.compose.LsDoDevTools

/**
 * Debug builds (Xcode's Debug configuration): screens register into an LS-DO registry served on
 * the device's loopback. The simulator shares the Mac's: `LSDO_URL=http://127.0.0.1:8765 scripts/lsdo ls`.
 */
internal fun startLsDo() {
    LsDoDevTools.start(catalog = LsDoScreens)
}
