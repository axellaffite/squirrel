package app.squirrel.ui.ios

import app.squirrel.api.models.DevicePlatform
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.auth.KeychainTokenStore
import app.squirrel.shared.db.createIosDriver
import app.squirrel.shared.ios.darwinEngine
import app.squirrel.ui.AppGraph
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.MainScope
import platform.Foundation.NSBundle
import platform.UIKit.UIDevice

/** Builds the iOS [AppGraph] once per process: SQLite, the Keychain, NSURLSession, PhotoKit. */
object IosGraph {
    private val scope = MainScope()
    private val clock = { kotlin.time.Clock.System.now().toEpochMilliseconds() }

    val backup: IosBackup by lazy {
        IosBackup(database = { instance.database }, settings = { instance.settings }, runner = GraphRunner { instance }, scope = scope, now = clock)
    }

    val instance: AppGraph by lazy { create() }

    private fun create(): AppGraph {
        val engine = darwinEngine()
        val device = UIDevice.currentDevice
        return AppGraph(
            driver = createIosDriver(),
            tokenStore = { KeychainTokenStore() },
            httpEngine = { engine },
            device = DeviceDetails(
                name = device.name,
                platform = DevicePlatform.ios,
                model = device.model,
                appVersion = NSBundle.mainBundle.objectForInfoDictionaryKey("CFBundleShortVersionString") as? String ?: "0",
            ),
            scope = scope,
            io = Dispatchers.IO,
            backup = backup,
            images = IosImages(),
            clock = clock,
        )
    }
}
