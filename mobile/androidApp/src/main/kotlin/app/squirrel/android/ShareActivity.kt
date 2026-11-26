package app.squirrel.android

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.core.content.IntentCompat
import app.squirrel.android.files.pickedFile
import app.squirrel.ui.send.SendApp

/**
 * "Squirrel" in the system share sheet, for any file (one or several): the files go into a folder
 * of one's own, then are shared with one person ([SendApp]). Sharing ends with this activity.
 */
class ShareActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) setRecentsScreenshotEnabled(false)
        val app = application as SquirrelApp
        val files = sharedUris(intent).map { pickedFile(contentResolver, it) }
        setContent { SendApp(app.graph, files, onClose = ::finish) }
    }
}

/** The content URIs an `ACTION_SEND` / `ACTION_SEND_MULTIPLE` carries, once each. */
private fun sharedUris(intent: Intent): List<Uri> {
    val streams = when (intent.action) {
        Intent.ACTION_SEND -> listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
        Intent.ACTION_SEND_MULTIPLE -> IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
        else -> emptyList()
    }
    // Some apps only fill the clip data, which also carries the read grant.
    val clip = intent.clipData?.let { data -> (0 until data.itemCount).mapNotNull { data.getItemAt(it).uri } }.orEmpty()
    return (streams + clip).filter { it.scheme == "content" || it.scheme == "file" }.distinct()
}
