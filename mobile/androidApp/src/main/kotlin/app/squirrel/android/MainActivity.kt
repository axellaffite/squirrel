package app.squirrel.android

import android.annotation.SuppressLint
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.result.contract.ActivityResultContracts
import app.squirrel.android.files.AndroidFilePicker
import app.squirrel.android.media.MediaPermissions
import app.squirrel.ui.backup.PhotoAccessActions
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import app.squirrel.android.sync.MediaObserver
import app.squirrel.ui.App
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val scope = MainScope()

    /** Registered with the activity, before it starts (as every activity result must be). */
    private val picker = AndroidFilePicker(this)
    private lateinit var observer: MediaObserver
    private val permissions = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { granted ->
        val app = application as SquirrelApp
        // A grant (full or partial) starts a run; a refusal leaves the screen's notice.
        if (granted.values.any { it }) app.graph.backUpNow()
        if (granted.isNotEmpty() && granted.values.none { it } && !shouldShowAnyRationale()) openAppSettings()
    }

    /** Android 13+: images and video; 14+ also "selected photos" (the system offers the picker). */
    private val photoActions = object : PhotoAccessActions {
        override fun requestAccess() = permissions.launch(MediaPermissions.toRequest(Build.VERSION.SDK_INT).toTypedArray())
        override fun changeSelection() = requestAccess()
        override fun allowAll() = openAppSettings()
        override fun openBatterySettings() = openBatterySettingsPage()
    }

    /**
     * The system's one-tap "stop optimising" dialog for Squirrel (the app is installed directly,
     * not from Play, whose policy limits this permission); some phones lack it: the app's page.
     */
    @SuppressLint("BatteryLife")
    private fun openBatterySettingsPage() {
        try {
            startActivity(Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.fromParts("package", packageName, null)))
        } catch (_: ActivityNotFoundException) {
            openAppSettings()
        }
    }

    private fun shouldShowAnyRationale() =
        MediaPermissions.toRequest(Build.VERSION.SDK_INT).any { shouldShowRequestPermissionRationale(it) }

    /** Permanently denied: only the app's settings page can grant it now. */
    private fun openAppSettings() {
        startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", packageName, null)))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        // Private photos and files: no snapshot of them in the recent apps screen.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) setRecentsScreenshotEnabled(false)
        val app = application as SquirrelApp
        observer = MediaObserver(this) {
            scope.launch(Dispatchers.IO) { app.backup.scheduler.onNewMedia(app.graph.settings.backup()) }
        }
        setContent { App(app.graph, photoActions, picker) }
    }

    override fun onStart() {
        super.onStart()
        (application as SquirrelApp).graph.onAppVisible()
        observer.register()
    }

    override fun onStop() {
        observer.unregister()
        super.onStop()
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}
