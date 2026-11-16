package app.squirrel.ui.backup

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.AssetSource
import app.squirrel.shared.sync.Conditions

/** What backup needs from the platform: the photo library, its conditions, its scheduler. */
interface BackupPlatform {
    val source: AssetSource
    val conditions: Conditions

    /** Albums for the settings screen (name and count). */
    suspend fun albums(): List<Pair<String, Int>>

    /** Re-arms background backups for [settings] (cancels them when automatic backup is off). */
    fun schedule(settings: BackupSettings)

    /** "Back up now": one run as soon as the constraints allow. */
    fun backUpNow(settings: BackupSettings)

    /** Signed out: no more background work. */
    fun cancel()
}
