package app.squirrel.ui.backup

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.AssetSource
import app.squirrel.shared.sync.Conditions

/** What backup needs from the platform: the photo library, its conditions, its scheduler. */
interface BackupPlatform {
    val source: AssetSource
    val conditions: Conditions

    /** Whether photo locations survive (Android: ACCESS_MEDIA_LOCATION granted). */
    fun locationsIncluded(): Boolean

    /** Albums for the settings screen (name and count). */
    suspend fun albums(): List<Pair<String, Int>>

    /** Re-arms background backups for [settings] (cancels them when automatic backup is off). */
    fun schedule(settings: BackupSettings)

    /** "Back up now": one run as soon as the constraints allow. */
    fun backUpNow(settings: BackupSettings)

    /** Signed out: no more background work. */
    fun cancel()
}

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
