package app.squirrel.android.sync

import android.content.Context
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore

/** While the app is in the foreground: new photos start a backup within seconds (debounced). */
class MediaObserver(private val context: Context, private val onChange: () -> Unit) {
    private val handler = Handler(Looper.getMainLooper())
    private val fire = Runnable { onChange() }
    private val observer = object : ContentObserver(handler) {
        override fun onChange(selfChange: Boolean) {
            handler.removeCallbacks(fire)
            handler.postDelayed(fire, DEBOUNCE_MS)
        }
    }

    fun register() {
        val resolver = context.contentResolver
        resolver.registerContentObserver(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, true, observer)
        resolver.registerContentObserver(MediaStore.Video.Media.EXTERNAL_CONTENT_URI, true, observer)
    }

    fun unregister() {
        handler.removeCallbacks(fire)
        context.contentResolver.unregisterContentObserver(observer)
    }

    private companion object {
        const val DEBOUNCE_MS = 5_000L
    }
}
