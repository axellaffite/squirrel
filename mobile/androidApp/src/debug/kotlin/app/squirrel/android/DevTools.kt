package app.squirrel.android

import android.util.Log
import app.squirrel.lsdo.LsDo
import app.squirrel.lsdo.runtime.LSDO_PORT
import app.squirrel.lsdo.runtime.LsDoRegistry
import app.squirrel.lsdo.runtime.startLsDoServer
import kotlinx.coroutines.Dispatchers

/**
 * Debug builds: screens register into an LS-DO registry served on the phone's loopback (port
 * [LSDO_PORT]); `scripts/lsdo` reaches it through `adb forward`. Release has an empty twin.
 */
internal object DevTools {
    fun start() {
        ComposeUiCapture.install()
        val registry = LsDoRegistry(context = Dispatchers.Main.immediate, capture = ComposeUiCapture::capture)
        LsDo.install(registry)
        runCatching { startLsDoServer(registry) }
            .onSuccess { Log.i("LsDo", "LS-DO listening on 127.0.0.1:$LSDO_PORT") }
            .onFailure { Log.w("LsDo", "LS-DO server did not start", it) }
    }
}
