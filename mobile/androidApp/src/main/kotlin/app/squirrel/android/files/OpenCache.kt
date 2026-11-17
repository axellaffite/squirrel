package app.squirrel.android.files

import java.io.File
import java.io.IOException
import java.util.UUID

/**
 * `cacheDir/open/<id>/<name>`: originals handed to other apps. Kept briefly (the other app may
 * still be reading), pruned at start and before each download, written through a temp file so
 * two downloads of the same id never see each other's half-written bytes.
 */
class OpenCache(private val dir: File, private val now: () -> Long = System::currentTimeMillis) {
    fun target(id: String, name: String): File = File(File(dir, safeId(id)), safeName(name))

    /** Deletes everything except [keepId]'s folder and what was written in the last hour. */
    fun prune(keepId: String? = null) {
        val keep = keepId?.let(::safeId)
        dir.listFiles()?.forEach { entry ->
            val recent = now() - newest(entry) < KEEP_MS
            if (entry.name != keep && !recent) entry.deleteRecursively()
        }
    }

    /** A fresh temp file next to [target]. */
    fun temp(target: File): File {
        target.parentFile?.mkdirs()
        return File(target.parentFile, ".${target.name}.${UUID.randomUUID()}.part")
    }

    /** Moves a complete download into place (a rename within one folder). */
    fun commit(temp: File, target: File) {
        if (target.exists()) target.delete()
        if (!temp.renameTo(target)) throw IOException("Could not move the download into place")
    }

    fun clear() {
        dir.deleteRecursively()
    }

    /** When its newest file was written (folders' own times change with every listing). */
    private fun newest(entry: File): Long =
        entry.walkTopDown().filter { it.isFile }.maxOfOrNull { it.lastModified() } ?: entry.lastModified()

    companion object {
        const val KEEP_MS = 60 * 60_000L

        private fun safeId(id: String) = id.filter { it.isLetterOrDigit() || it == '-' }.ifEmpty { "file" }

        /** A file name other apps can show; never a path. */
        fun safeName(name: String): String =
            name.replace('/', '_').replace('\\', '_').trim().trimStart('.').ifBlank { "file" }.take(120)
    }
}
