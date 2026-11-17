package app.squirrel.ui.backup

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.AssetSource
import app.squirrel.shared.sync.ChunkTransport
import app.squirrel.shared.sync.Conditions
import app.squirrel.shared.api.SquirrelApi

/** What backup needs from the platform: the photo library, its conditions, its scheduler. */
interface BackupPlatform {
    val source: AssetSource
    val conditions: Conditions

    /** Whether photo locations survive (Android: ACCESS_MEDIA_LOCATION granted). */
    fun locationsIncluded(): Boolean

    /** Albums for the settings screen. */
    suspend fun albums(): List<AlbumChoice>

    /** Re-arms background backups for [settings] (cancels them when automatic backup is off). */
    fun schedule(settings: BackupSettings)

    /** A background run soon (photo access changed, new media while open), if automatic backup is on. */
    fun runSoon(settings: BackupSettings)

    /** "Back up now": one run as soon as the constraints allow. */
    fun backUpNow(settings: BackupSettings)

    /** Signed out: no more background work. */
    fun cancel()

    /** How chunks travel for [api] (iOS: the background session); null: Ktor with the bearer token. */
    fun transport(api: SquirrelApi): ChunkTransport? = null
}

/** One album: [id] is what the filter stores (Android BUCKET_ID), [name] what is shown. */
data class AlbumChoice(val id: String, val name: String, val count: Int)

/** Platform dialogs the backup screens open. */
interface PhotoAccessActions {
    /** The system permission dialog (also "Allow all photos"). */
    fun requestAccess()

    /** Android 14+ / iOS: the picker of selected photos. */
    fun changeSelection()

    /** From partial access to all photos: the system settings page (a new request only re-opens the picker). */
    fun allowAll()

    /** No-op default for previews and tests. */
    object None : PhotoAccessActions {
        override fun requestAccess() = Unit
        override fun changeSelection() = Unit
        override fun allowAll() = Unit
    }
}
