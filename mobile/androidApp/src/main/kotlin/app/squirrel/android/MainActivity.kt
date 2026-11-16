package app.squirrel.android

import android.os.Bundle
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
    private lateinit var observer: MediaObserver

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val app = application as SquirrelApp
        observer = MediaObserver(this) {
            scope.launch(Dispatchers.IO) { app.backup.scheduler.onNewMedia(app.graph.settings.backup()) }
        }
        setContent { App(app.graph) }
    }

    override fun onStart() {
        super.onStart()
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
