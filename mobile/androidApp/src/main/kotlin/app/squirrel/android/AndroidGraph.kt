package app.squirrel.android

import android.content.Context
import android.os.Build
import app.squirrel.api.models.DevicePlatform
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.auth.KeystoreTokenStore
import app.squirrel.shared.db.createAndroidDriver
import app.squirrel.ui.AppGraph
import io.ktor.client.engine.okhttp.OkHttp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/** Builds the Android [AppGraph]: SQLite database, Keystore token store, OkHttp. */
object AndroidGraph {
    fun create(context: Context): AppGraph {
        val engine = OkHttp.create()
        return AppGraph(
            driver = createAndroidDriver(context),
            tokenStore = ::KeystoreTokenStore,
            engine = { engine },
            device = DeviceDetails(
                name = Build.MODEL,
                platform = DevicePlatform.android,
                model = Build.MODEL,
                appVersion = BuildConfig.VERSION_NAME,
            ),
            allowHttp = BuildConfig.DEBUG,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        )
    }
}
