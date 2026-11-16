package app.squirrel.android.media

import android.content.ContentResolver
import android.content.ContentUris
import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import android.provider.MediaStore.MediaColumns
import app.squirrel.shared.db.ScannedAsset
import app.squirrel.shared.sync.AssetReader
import app.squirrel.shared.sync.AssetSource
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.shared.sync.ScanResult
import app.squirrel.shared.sync.SourceFilter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.FileNotFoundException
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.temporal.ChronoUnit

private const val IMAGES = "images"
private const val VIDEO = "video"
private const val MILLIS = 1000L
/** API 29 has no generations: rescan from the newest modification time, with an overlap. */
private const val DATE_OVERLAP_SECONDS = 2L

/**
 * The phone's photos and videos. Ids are `images:<_ID>` / `video:<_ID>` (two id spaces).
 * Incremental scans use MediaStore generations (API 30+; a new volume version means a full
 * rescan) or modification times (API 29). Deletions are not listed: an asset gone from the
 * phone is skipped when it cannot be opened. Pending and trashed items are excluded by default.
 */
class MediaStoreSource(context: Context, private val access: MediaAccessReader) : AssetSource {
    private val app = context.applicationContext
    private val resolver: ContentResolver = app.contentResolver

    private data class Kind(val prefix: String, val kind: String, val uri: Uri)

    private val images = Kind(IMAGES, SourceFilter.KIND_PHOTO, MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL))
    private val videos = Kind(VIDEO, SourceFilter.KIND_VIDEO, MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL))

    override suspend fun changes(token: String?, filter: SourceFilter): ScanResult = withContext(Dispatchers.IO) {
        val since = token?.let(::parseToken)?.takeIf { it.version == volumeVersion() }
        val kinds = if (filter.includeVideos) listOf(images, videos) else listOf(images)
        var newest = since?.position ?: 0L
        val assets = kinds.flatMap { kind ->
            query(kind, since).also { rows -> rows.forEach { newest = maxOf(newest, it.position) } }.map { it.asset }
        }
        ScanResult(assets.toList(), removed = emptyList(), token = Token(volumeVersion(), newest).encode(), full = since == null)
    }

    override suspend fun open(deviceAssetId: String): AssetReader? = withContext(Dispatchers.IO) {
        val uri = uriOf(deviceAssetId) ?: return@withContext null
        val modified = resolver.query(uri, arrayOf(MediaColumns.DATE_MODIFIED), null, null, null)?.use {
            if (it.moveToFirst()) it.getLong(0) * MILLIS else null
        } ?: return@withContext null
        // Without ACCESS_MEDIA_LOCATION the original (with GPS) cannot be read; the redacted copy is.
        val target = if (access.locationsIncluded()) MediaStore.setRequireOriginal(uri) else uri
        try {
            val descriptor = resolver.openFileDescriptor(target, "r") ?: return@withContext null
            MediaStoreReader(descriptor, descriptor.statSize, modified)
        } catch (_: FileNotFoundException) {
            null
        } catch (_: SecurityException) {
            null // No longer visible (partial access shrank, permission revoked).
        } catch (_: IllegalArgumentException) {
            null
        }
    }

    override fun access(): MediaAccess = access.current()

    /** Buckets with their photo and video counts, for the album filter. */
    suspend fun albums(): List<Album> = withContext(Dispatchers.IO) {
        listOf(images, videos).flatMap { kind ->
            resolver.query(kind.uri, arrayOf(MediaColumns.BUCKET_DISPLAY_NAME), null, null, null)?.use { cursor ->
                buildList { while (cursor.moveToNext()) add(cursor.getString(0) ?: "") }
            }.orEmpty()
        }.filter { it.isNotEmpty() }.groupingBy { it }.eachCount()
            .map { (name, count) -> Album(name, count) }.sortedBy { it.name.lowercase() }
    }

    private data class Row(val asset: ScannedAsset, val position: Long)

    private fun query(kind: Kind, since: Token?): List<Row> {
        val generations = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
        val projection = buildList {
            addAll(listOf(MediaColumns._ID, MediaColumns.DISPLAY_NAME, MediaColumns.MIME_TYPE, MediaColumns.SIZE,
                MediaColumns.DATE_TAKEN, MediaColumns.DATE_MODIFIED, MediaColumns.BUCKET_DISPLAY_NAME))
            if (generations) add(MediaColumns.GENERATION_MODIFIED)
        }.toTypedArray()
        val (selection, args) = when {
            since == null -> null to null
            generations -> "${MediaColumns.GENERATION_MODIFIED} > ?" to arrayOf(since.position.toString())
            else -> "${MediaColumns.DATE_MODIFIED} >= ?" to arrayOf((since.position - DATE_OVERLAP_SECONDS).toString())
        }
        return resolver.query(kind.uri, projection, selection, args, null)?.use { cursor ->
            buildList { while (cursor.moveToNext()) row(kind, cursor, generations)?.let(::add) }
        }.orEmpty()
    }

    private fun row(kind: Kind, c: Cursor, generations: Boolean): Row? {
        val size = c.getLong(3)
        if (size <= 0) return null
        val modifiedSeconds = c.getLong(5)
        val asset = ScannedAsset(
            deviceAssetId = "${kind.prefix}:${c.getLong(0)}",
            kind = kind.kind,
            name = c.getString(1) ?: "${kind.prefix}-${c.getLong(0)}",
            mime = c.getString(2),
            size = size,
            modifiedAt = modifiedSeconds * MILLIS,
            takenAt = if (c.isNull(4)) null else takenAt(c.getLong(4)),
            album = c.getString(6),
        )
        return Row(asset, if (generations) c.getLong(7) else modifiedSeconds)
    }

    private fun uriOf(id: String): Uri? {
        val number = id.substringAfter(':').toLongOrNull() ?: return null
        val base = when (id.substringBefore(':')) {
            IMAGES -> images.uri
            VIDEO -> videos.uri
            else -> return null
        }
        return ContentUris.withAppendedId(base, number)
    }

    private fun volumeVersion(): String =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) MediaStore.getVersion(app) else "dm"

    private data class Token(val version: String, val position: Long) {
        fun encode() = "$version|$position"
    }

    private fun parseToken(token: String): Token? {
        val version = token.substringBeforeLast('|', missingDelimiterValue = "")
        val position = token.substringAfterLast('|').toLongOrNull() ?: return null
        return Token(version, position)
    }
}

/** DATE_TAKEN (epoch ms) as RFC 3339 with the phone's offset at that instant; null when unknown. */
fun takenAt(epochMillis: Long, zone: ZoneId = ZoneId.systemDefault()): String? {
    if (epochMillis <= 0) return null
    return Instant.ofEpochMilli(epochMillis).atZone(zone).toOffsetDateTime()
        .truncatedTo(ChronoUnit.SECONDS)
        .format(DateTimeFormatter.ISO_OFFSET_DATE_TIME)
}
