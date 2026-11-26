package app.squirrel.android

import android.content.Context
import android.os.Build
import app.squirrel.android.files.AndroidFileOpener
import app.squirrel.android.files.AndroidPhotoSaver
import app.squirrel.android.sync.AndroidBackup
import app.squirrel.api.models.DevicePlatform
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.auth.KeystoreTokenStore
import app.squirrel.shared.db.createAndroidDriver
import app.squirrel.ui.AppGraph
import io.ktor.client.engine.okhttp.OkHttp
import kotlinx.coroutines.CoroutineScope

/** Builds the Android [AppGraph]: SQLite database, Keystore token store, OkHttp, MediaStore backup. */
object AndroidGraph {
    fun create(context: Context, backup: AndroidBackup, scope: CoroutineScope): AppGraph {
        val engine = OkHttp.create()
        return AppGraph(
            driver = createAndroidDriver(context),
            tokenStore = ::KeystoreTokenStore,
            httpEngine = { engine },
            device = DeviceDetails(
                name = Build.MODEL,
                platform = DevicePlatform.android,
                model = Build.MODEL,
                appVersion = BuildConfig.VERSION_NAME,
            ),
            scope = scope,
            backup = backup,
            images = AndroidImages(context),
            files = AndroidFileOpener(context).also { it.prune() },
            saver = AndroidPhotoSaver(context),
        )
    }
}
