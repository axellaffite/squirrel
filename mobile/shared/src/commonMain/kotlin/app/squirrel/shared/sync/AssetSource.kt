package app.squirrel.shared.sync

import app.squirrel.shared.db.ScannedAsset
import app.squirrel.shared.settings.AlbumFilter

/** What the user chose to back up. */
data class SourceFilter(val includeVideos: Boolean, val albums: AlbumFilter) {
    fun accepts(asset: ScannedAsset): Boolean =
        (includeVideos || asset.kind != KIND_VIDEO) &&
            when (albums) {
                AlbumFilter.All -> true
                is AlbumFilter.Selected -> asset.album in albums.names
            }

    /** Changes whenever the selection changes, to force a full rescan. */
    fun fingerprint(): String = when (albums) {
        AlbumFilter.All -> "videos=$includeVideos;albums=*"
        is AlbumFilter.Selected -> "videos=$includeVideos;albums=" + albums.names.sorted().joinToString("\u0000")
    }

    companion object {
        const val KIND_PHOTO = "photo"
        const val KIND_VIDEO = "video"
    }
}

/**
 * One scan's answer. [full]: [assets] is everything visible (a rescan); otherwise only what
 * changed since the token, plus the ids [removed] from the phone (or no longer visible).
 */
data class ScanResult(
    val assets: List<ScannedAsset>,
    val removed: List<String>,
    val token: String?,
    val full: Boolean,
)

/** How much of the photo library the app may read. */
sealed interface MediaAccess {
    data object Full : MediaAccess
    /** Android 14+ "Select photos", iOS "Limited Access": only these are backed up. */
    data class Partial(val count: Int) : MediaAccess
    /** All photos, but not videos (Android 13+ grants them separately). */
    data object NoVideos : MediaAccess
    data object None : MediaAccess
}

/** The phone's photo library: MediaStore on Android, PhotoKit on iOS. */
interface AssetSource {
    /** Assets changed since [token] (null: everything visible), and the token to store next. */
    suspend fun changes(token: String?, filter: SourceFilter): ScanResult

    /** Null when the asset is gone from the phone. */
    suspend fun open(deviceAssetId: String): AssetReader?

    fun access(): MediaAccess

    /**
     * Changes whenever what the app may see changes (full, partial and which items, videos or
     * not): a change forces a full rescan, so newly visible assets are found.
     */
    fun accessFingerprint(): String = access().toString()
}
