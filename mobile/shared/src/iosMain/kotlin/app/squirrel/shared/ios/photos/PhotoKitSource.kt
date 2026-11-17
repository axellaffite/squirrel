package app.squirrel.shared.ios.photos

import app.squirrel.shared.db.ScannedAsset
import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.sync.AssetReader
import app.squirrel.shared.sync.AssetSource
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.shared.sync.ScanResult
import app.squirrel.shared.sync.SourceFilter
import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.withContext
import platform.Foundation.NSKeyedArchiver
import platform.Foundation.NSKeyedUnarchiver
import platform.Foundation.NSPredicate
import platform.Foundation.NSTimeZone
import platform.Foundation.localTimeZone
import platform.Foundation.timeIntervalSince1970
import platform.Photos.PHAsset
import platform.Photos.PHAssetCollection
import platform.Photos.PHAssetCollectionSubtypeAny
import platform.Photos.PHAssetCollectionTypeAlbum
import platform.Photos.PHAssetMediaTypeImage
import platform.Photos.PHAssetMediaTypeVideo
import platform.Photos.PHAssetResource
import platform.Photos.PHFetchOptions
import platform.Photos.PHFetchResult
import platform.Photos.PHObjectTypeAsset
import platform.Photos.PHPersistentChangeToken
import platform.Photos.PHPhotoLibrary

/** One user album, for the settings screen. */
data class PhotoAlbum(val id: String, val name: String, val count: Int)

/**
 * The photo library through PhotoKit. Sizes are known only after export, so scans report -1 and
 * iOS change detection relies on the modification date alone.
 */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class PhotoKitSource(
    private val exporter: AssetExporter = AssetExporter(),
    /** iCloud originals may be downloaded (false: "Only on Wi-Fi" and on a metered network now). */
    private val allowNetwork: () -> Boolean = { true },
) : AssetSource {
    private val library get() = PHPhotoLibrary.sharedPhotoLibrary()

    override suspend fun changes(token: String?, filter: SourceFilter): ScanResult = withContext(Dispatchers.IO) {
        exporter.sweep()
        val next = currentToken()
        // Selected albums: membership is per collection, so those scans are always full (small).
        val changed = if (token != null && filter.albums == AlbumFilter.All) changedIds(token) else null
        if (changed == null) {
            ScanResult(assets = all(filter), removed = emptyList(), token = next, full = true)
        } else {
            // Deleted ids are ignored: backup only adds, and `existing` answers what is gone.
            val assets = if (changed.isEmpty()) emptyList() else scanned(PHAsset.fetchAssetsWithLocalIdentifiers(changed, options(filter)), null)
            ScanResult(assets = assets, removed = emptyList(), token = next, full = false)
        }
    }

    override suspend fun open(deviceAssetId: String): AssetReader? = withContext(Dispatchers.IO) {
        val asset = PHAsset.fetchAssetsWithLocalIdentifiers(listOf(deviceAssetId), null).firstObject as? PHAsset
            ?: return@withContext null
        val resource = primary(asset) ?: return@withContext null
        val modified = modifiedAt(asset)
        val path = exporter.export(deviceAssetId, modified, resource, allowNetwork())
        exporter.reader(path, modified)
    }

    override suspend fun existing(ids: List<String>): Set<String> = withContext(Dispatchers.IO) {
        if (ids.isEmpty()) return@withContext emptySet()
        val found = PHAsset.fetchAssetsWithLocalIdentifiers(ids, null)
        (0 until found.count.toInt()).mapNotNullTo(HashSet()) { (found.objectAtIndex(it.toULong()) as? PHAsset)?.localIdentifier }
    }

    /** The server has it: its export is no longer needed. */
    override fun backedUp(deviceAssetId: String) = exporter.forget(deviceAssetId)

    override fun access(): MediaAccess = PhotoAccess.current()

    /** Limited access: which photos are visible matters too (a reselection rescans). */
    override fun accessFingerprint(): String = when (val access = access()) {
        is MediaAccess.Partial -> {
            val visible = PHAsset.fetchAssetsWithOptions(null)
            val ids = (0 until visible.count.toInt()).mapNotNull { (visible.objectAtIndex(it.toULong()) as? PHAsset)?.localIdentifier }
            "partial:" + ids.sorted().joinToString(",").hashCode()
        }
        else -> access.toString()
    }

    /** The user's albums. */
    fun albums(): List<PhotoAlbum> {
        val collections = PHAssetCollection.fetchAssetCollectionsWithType(PHAssetCollectionTypeAlbum, PHAssetCollectionSubtypeAny, null)
        return (0 until collections.count.toInt()).mapNotNull { index ->
            val collection = collections.objectAtIndex(index.toULong()) as? PHAssetCollection ?: return@mapNotNull null
            PhotoAlbum(
                id = collection.localIdentifier,
                name = collection.localizedTitle ?: "Album",
                count = PHAsset.fetchAssetsInAssetCollection(collection, null).count.toInt(),
            )
        }
    }

    private fun all(filter: SourceFilter): List<ScannedAsset> = when (val albums = filter.albums) {
        AlbumFilter.All -> scanned(PHAsset.fetchAssetsWithOptions(options(filter)), null)
        is AlbumFilter.Selected -> {
            val collections = PHAssetCollection.fetchAssetCollectionsWithLocalIdentifiers(albums.names.toList(), null)
            (0 until collections.count.toInt()).flatMap { index ->
                val collection = collections.objectAtIndex(index.toULong()) as PHAssetCollection
                scanned(PHAsset.fetchAssetsInAssetCollection(collection, options(filter)), collection.localIdentifier)
            }.distinctBy { it.deviceAssetId }
        }
    }

    /** Inserted or updated asset ids since [token]; null when the token is unusable (rescan). */
    private fun changedIds(token: String): List<String>? {
        val data = AssetMapping.decode(token) ?: return null
        val since = NSKeyedUnarchiver.unarchivedObjectOfClass(PHPersistentChangeToken, data, null) as? PHPersistentChangeToken
            ?: return null
        // An expired token (PHPhotosErrorPersistentChangeTokenExpired) or any other error: rescan.
        val result = library.fetchPersistentChangesSinceToken(since, null) ?: return null
        val ids = LinkedHashSet<String>()
        result.enumerateChangesWithBlock { change, _ ->
            val details = change?.changeDetailsForObjectType(PHObjectTypeAsset, null) ?: return@enumerateChangesWithBlock
            details.insertedLocalIdentifiers.forEach { ids += it as String }
            details.updatedLocalIdentifiers.forEach { ids += it as String }
        }
        return ids.toList()
    }

    private fun currentToken(): String? {
        val data = NSKeyedArchiver.archivedDataWithRootObject(library.currentChangeToken, requiringSecureCoding = true, error = null)
        return data?.let(AssetMapping::encode)
    }

    private fun options(filter: SourceFilter) = PHFetchOptions().apply {
        includeHiddenAssets = false
        predicate = if (filter.includeVideos) {
            NSPredicate.predicateWithFormat("mediaType == %d OR mediaType == %d", PHAssetMediaTypeImage, PHAssetMediaTypeVideo)
        } else {
            NSPredicate.predicateWithFormat("mediaType == %d", PHAssetMediaTypeImage)
        }
    }

    private fun scanned(result: PHFetchResult, album: String?): List<ScannedAsset> =
        (0 until result.count.toInt()).mapNotNull { index ->
            val asset = result.objectAtIndex(index.toULong()) as? PHAsset ?: return@mapNotNull null
            val resource = primary(asset) ?: return@mapNotNull null
            val created = asset.creationDate
            ScannedAsset(
                deviceAssetId = asset.localIdentifier,
                kind = if (asset.mediaType == PHAssetMediaTypeVideo) "video" else "photo",
                name = resource.originalFilename,
                mime = AssetMapping.mime(resource.uniformTypeIdentifier, resource.originalFilename),
                size = -1,
                modifiedAt = modifiedAt(asset),
                takenAt = created?.let {
                    AssetMapping.takenAt(it.timeIntervalSince1970, NSTimeZone.localTimeZone.secondsFromGMTForDate(it).toInt())
                },
                album = album,
            )
        }

    private fun primary(asset: PHAsset): PHAssetResource? {
        val resources = PHAssetResource.assetResourcesForAsset(asset).map { it as PHAssetResource }
        val index = AssetMapping.primaryResource(resources.map { it.type }, video = asset.mediaType == PHAssetMediaTypeVideo)
        return index?.let(resources::get)
    }

    private fun modifiedAt(asset: PHAsset): Long =
        ((asset.modificationDate ?: asset.creationDate)?.timeIntervalSince1970 ?: 0.0).times(1000).toLong()
}
