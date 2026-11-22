package app.squirrel.ui.backup

import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.AssetSource
import app.squirrel.shared.sync.ChunkTransport
import app.squirrel.shared.sync.Conditions
import app.squirrel.shared.sync.NetworkWaiter
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon

/** What backup needs from the platform: the photo library, its conditions, its scheduler. */
interface BackupPlatform {
    val source: AssetSource
    val conditions: Conditions

    /** Whether photo locations survive (Android: ACCESS_MEDIA_LOCATION granted). */
    fun locationsIncluded(): Boolean

    /** The system may stop backups while the screen is off (Android: battery optimisation is on for Squirrel). */
    fun batteryRestricted(): Boolean = false

    /** What the phone groups photos in, and what the screens call them. */
    val albumKind: AlbumKind get() = AlbumKind.Albums

    /** The groups backup can be limited to (Android folders, iOS albums). */
    suspend fun albums(): List<AlbumChoice>

    /** Re-arms background backups for [settings] (cancels them when automatic backup is off). */
    fun schedule(settings: BackupSettings)

    /** A background run soon (photo access changed, new media while open), if automatic backup is on. */
    fun runSoon(settings: BackupSettings)

    /** "Back up now": one run as soon as the constraints allow. */
    fun backUpNow(settings: BackupSettings)

    /** Signed out: no more background work. */
    fun cancel()

    /** What wakes a foreground run waiting for the network; null: plain timed probes. */
    fun networkWaiter(): NetworkWaiter? = null

    /** How chunks travel for [api] (iOS: the background session); null: Ktor with the bearer token. */
    fun transport(api: SquirrelApi): ChunkTransport? = null
}

/**
 * One album or folder: [id] is what the filter stores (Android BUCKET_ID, iOS local identifier),
 * [name] what is shown, [path] where it lives when the platform has one ("DCIM/Camera").
 */
data class AlbumChoice(val id: String, val name: String, val count: Int, val path: String? = null)

/** Android backs up folders (MediaStore buckets); iOS, PhotoKit albums. */
enum class AlbumKind(val title: String, val one: String, val many: String, val icon: SquirrelIcon) {
    Folders("Folders", "folder", "folders", SquirrelIcon.Folder),
    Albums("Albums", "album", "albums", SquirrelIcon.Library),
}

/** Platform dialogs the backup screens open. */
interface PhotoAccessActions {
    /** The system permission dialog (also "Allow all photos"). */
    fun requestAccess()

    /** Android 14+ / iOS: the picker of selected photos. */
    fun changeSelection()

    /** From partial access to all photos: the system settings page (a new request only re-opens the picker). */
    fun allowAll()

    /** Android: the system's "allow background use" (battery optimisation) for Squirrel. */
    fun openBatterySettings() = Unit

    /** No-op default for previews and tests. */
    object None : PhotoAccessActions {
        override fun requestAccess() = Unit
        override fun changeSelection() = Unit
        override fun allowAll() = Unit
    }
}
