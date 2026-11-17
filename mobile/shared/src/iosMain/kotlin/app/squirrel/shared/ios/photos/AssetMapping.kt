package app.squirrel.shared.ios.photos

import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.datetime.TimeZone
import kotlinx.datetime.UtcOffset
import kotlinx.datetime.toLocalDateTime
import platform.Foundation.NSData
import platform.Foundation.base64EncodedStringWithOptions
import platform.Foundation.create
import platform.Photos.PHAssetResourceTypeFullSizePhoto
import platform.Photos.PHAssetResourceTypeFullSizeVideo
import platform.Photos.PHAssetResourceTypePhoto
import platform.Photos.PHAssetResourceTypeVideo
import platform.UniformTypeIdentifiers.UTType
import kotlin.time.Instant

/** Pure mapping from PhotoKit's answers to what the backup queue stores. */
object AssetMapping {
    /**
     * Which of an asset's resources (their `PHAssetResourceType` codes, in order) is the file to
     * back up: the edited version when there is one (what the user sees), else the original.
     * Live Photos: the still only (its paired video is never chosen). Null when none fits.
     */
    fun primaryResource(types: List<Long>, video: Boolean): Int? {
        val preferred = if (video) {
            listOf(PHAssetResourceTypeFullSizeVideo, PHAssetResourceTypeVideo)
        } else {
            listOf(PHAssetResourceTypeFullSizePhoto, PHAssetResourceTypePhoto)
        }
        return preferred.firstNotNullOfOrNull { wanted -> types.indexOf(wanted).takeIf { it >= 0 } }
    }

    private val byExtension = mapOf(
        "heic" to "image/heic", "heif" to "image/heif", "jpg" to "image/jpeg", "jpeg" to "image/jpeg",
        "png" to "image/png", "gif" to "image/gif", "webp" to "image/webp", "tif" to "image/tiff",
        "tiff" to "image/tiff", "dng" to "image/x-adobe-dng", "mov" to "video/quicktime",
        "mp4" to "video/mp4", "m4v" to "video/x-m4v",
    )

    /**
     * The MIME type for a uniform type identifier ("public.heic" → "image/heic"), else from the
     * file name's extension (types the system does not know, or no type database at hand).
     */
    fun mime(uti: String?, name: String): String? =
        uti?.let { UTType.typeWithIdentifier(it)?.preferredMIMEType }
            ?: byExtension[name.substringAfterLast('.', "").lowercase()]

    /** RFC 3339 local time with the phone's UTC offset at that moment: "2024-07-14T18:30:05+02:00". */
    fun takenAt(epochSeconds: Double, offsetSeconds: Int): String {
        val instant = Instant.fromEpochMilliseconds((epochSeconds * 1000).toLong())
        val offset = UtcOffset(seconds = offsetSeconds)
        val local = instant.toLocalDateTime(TimeZone.of(offset.toString().let { if (it == "Z") "UTC" else it }))
        val time = local.toString().substringBefore('.').let { if (it.length == 16) "$it:00" else it }
        return "$time${if (offsetSeconds == 0) "+00:00" else offset.toString()}"
    }

    /** Persistent change tokens are archived to bytes, stored as base64 text. */
    @OptIn(BetaInteropApi::class)
    fun encode(data: NSData): String = data.base64EncodedStringWithOptions(0u)

    @OptIn(BetaInteropApi::class)
    fun decode(text: String): NSData? = NSData.create(base64EncodedString = text, options = 0u)
}
