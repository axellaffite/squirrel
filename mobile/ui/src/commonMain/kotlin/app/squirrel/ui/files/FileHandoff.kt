package app.squirrel.ui.files

import app.squirrel.shared.api.SquirrelApi

/** What became of a hand-off. */
sealed interface HandoffResult {
    data object Opened : HandoffResult
    /** No app on the phone opens this kind of file. */
    data object NoApp : HandoffResult
    data class Failed(val message: String) : HandoffResult
}

/**
 * Downloads a file's original into the app's private cache and hands it to another app (view or
 * share). Platform-specific: Android uses a FileProvider URI.
 */
interface FileHandoff {
    suspend fun open(
        api: SquirrelApi,
        id: String,
        name: String,
        mime: String?,
        share: Boolean,
        onProgress: (Float) -> Unit,
    ): HandoffResult

    /** Deletes every downloaded file (sign-out). */
    fun clear()
}

/** A file name safe to use as one path segment: no separators, no leading dots, never empty. */
fun safeFileName(name: String): String =
    name.replace('/', '_').replace('\\', '_').trim().trimStart('.').ifEmpty { "file" }
