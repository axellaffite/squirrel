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
 * Incremental scans keep one cursor per volume: the MediaStore generation (API 30+; a new volume
 * version rescans that volume) or max(DATE_ADDED, DATE_MODIFIED) (API 29). Deletions are not listed: an asset gone from the
 * phone is skipped when it cannot be opened. Pending and trashed items are excluded by default.
 * `taken_at` is DATE_TAKEN, or the modification time when MediaStore has none.
 */
class MediaStoreSource(context: Context, private val access: MediaAccessReader) : AssetSource {
    private val app = context.applicationContext
    private val resolver: ContentResolver = app.contentResolver

    private data class Kind(val prefix: String, val kind: String, val uri: Uri, val uriFor: (String) -> Uri)

    private val images = Kind(IMAGES, SourceFilter.KIND_PHOTO, MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL)) {
        MediaStore.Images.Media.getContentUri(it)
    }
    private val videos = Kind(VIDEO, SourceFilter.KIND_VIDEO, MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL)) {
        MediaStore.Video.Media.getContentUri(it)
    }

    override suspend fun changes(token: String?, filter: SourceFilter): ScanResult = withContext(Dispatchers.IO) {
        val previous = decodeCursors(token)
        val generations = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
        val kinds = if (filter.includeVideos) listOf(images, videos) else listOf(images)
        val assets = mutableListOf<ScannedAsset>()
        val next = mutableMapOf<String, VolumeCursor>()
        var everyVolumeFull = true
        for (volume in MediaStore.getExternalVolumeNames(app).sorted()) {
            val version = if (generations) MediaStore.getVersion(app, volume) else API29_VERSION
            val since = previous[volume]?.takeIf { it.version == version }
            if (since != null) everyVolumeFull = false
            // The snapshot comes first: both kinds are read up to it, and anything newer is
            // picked up by the next scan (no change slips between the images and videos queries).
            val upTo = if (generations) MediaStore.getGeneration(app, volume) else null
            val rows = kinds.flatMap { kind -> query(kind, volume, since?.position, upTo) }
            assets += rows.map { it.asset }
            next[volume] = VolumeCursor(version, nextPosition(since?.position, upTo, rows.map { it.position }))
        }
        ScanResult(assets, removed = emptyList(), token = encodeCursors(next), full = everyVolumeFull)
    }

    override suspend fun open(deviceAssetId: String): AssetReader? = withContext(Dispatchers.IO) {
        val uri = uriOf(deviceAssetId) ?: return@withContext null
        try {
            val modified = resolver.query(uri, arrayOf(MediaColumns.DATE_MODIFIED), null, null, null)?.use {
                if (it.moveToFirst()) it.getLong(0) * MILLIS else null
            } ?: return@withContext null
            // The original (with GPS) needs ACCESS_MEDIA_LOCATION; when it is refused, the redacted
            // copy is backed up rather than nothing, and the asset says its location was stripped.
            val original = if (access.locationsIncluded()) openOrNull(MediaStore.setRequireOriginal(uri)) else null
            val descriptor = original ?: resolver.openFileDescriptor(uri, "r") ?: return@withContext null
            MediaStoreReader(descriptor, descriptor.statSize, modified, locationStripped = original == null)
        } catch (_: FileNotFoundException) {
            null
        } catch (_: SecurityException) {
            null // No longer visible (partial access shrank, permission revoked).
        } catch (_: IllegalArgumentException) {
            null
        }
    }

    private fun openOrNull(uri: Uri) = try {
        resolver.openFileDescriptor(uri, "r")
    } catch (_: SecurityException) {
        null
    } catch (_: UnsupportedOperationException) {
        null
    }

    override suspend fun existing(ids: List<String>): Set<String> = withContext(Dispatchers.IO) {
        ids.groupBy { it.substringBefore(':') }.flatMap { (prefix, group) ->
            val kind = when (prefix) {
                IMAGES -> images
                VIDEO -> videos
                else -> return@flatMap emptyList<String>()
            }
            group.chunked(EXISTING_CHUNK).flatMap { chunk ->
                val numbers = chunk.mapNotNull { it.substringAfter(':').toLongOrNull() }
                val selection = "${MediaColumns._ID} IN (${numbers.joinToString(",") { "?" }})"
                resolver.query(kind.uri, arrayOf(MediaColumns._ID), selection, numbers.map(Long::toString).toTypedArray(), null)
                    ?.use { c -> buildList { while (c.moveToNext()) add("$prefix:${c.getLong(0)}") } }
                    .orEmpty()
            }
        }.toSet()
    }

    override fun access(): MediaAccess = access.current()

    /** Full and None by kind; a partial grant by the exact set of selected items. */
    override fun accessFingerprint(): String = when (val current = access.current()) {
        is MediaAccess.Partial -> "partial|" + visibleIds().joinToString(",").hashCode().toString(16)
        else -> current.toString() + "|videos=" + access.videosGranted()
    }

    private fun visibleIds(): List<String> = listOf(images, videos).flatMap { kind ->
        resolver.query(kind.uri, arrayOf(MediaColumns._ID), null, null, "${MediaColumns._ID} ASC")?.use { c ->
            buildList { while (c.moveToNext()) add("${kind.prefix}:${c.getLong(0)}") }
        }.orEmpty()
    }

    /** Buckets with their photo and video counts, for the album filter (by id, per volume). */
    suspend fun albums(): List<Album> = withContext(Dispatchers.IO) {
        val found = mutableMapOf<String, Album>()
        for (volume in MediaStore.getExternalVolumeNames(app)) {
            for (kind in listOf(images, videos)) {
                val projection = arrayOf(MediaColumns.BUCKET_ID, MediaColumns.BUCKET_DISPLAY_NAME)
                resolver.query(kind.uriFor(volume), projection, null, null, null)?.use { c ->
                    while (c.moveToNext()) {
                        val id = c.getString(0) ?: continue
                        val name = c.getString(1)?.takeIf { it.isNotEmpty() } ?: continue
                        val current = found[id] ?: Album(id, name, volume, 0)
                        found[id] = current.copy(count = current.count + 1)
                    }
                }
            }
        }
        labelled(found.values.toList())
    }

    private data class Row(val asset: ScannedAsset, val position: Long)

    private fun query(kind: Kind, volume: String, since: Long?, upTo: Long?): List<Row> {
        val generations = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
        val projection = buildList {
            addAll(listOf(MediaColumns._ID, MediaColumns.DISPLAY_NAME, MediaColumns.MIME_TYPE, MediaColumns.SIZE,
                MediaColumns.DATE_TAKEN, MediaColumns.DATE_MODIFIED, MediaColumns.BUCKET_ID, MediaColumns.DATE_ADDED))
            if (generations) add(MediaColumns.GENERATION_MODIFIED)
        }.toTypedArray()
        val (selection, args) = when {
            generations && since != null ->
                "${MediaColumns.GENERATION_MODIFIED} > ? AND ${MediaColumns.GENERATION_MODIFIED} <= ?" to
                    arrayOf(since.toString(), checkNotNull(upTo).toString())
            generations -> "${MediaColumns.GENERATION_MODIFIED} <= ?" to arrayOf(checkNotNull(upTo).toString())
            since != null -> {
                // API 29: DATE_ADDED catches files copied in with an old modification time.
                val from = (since - DATE_OVERLAP_SECONDS).toString()
                "(${MediaColumns.DATE_ADDED} >= ? OR ${MediaColumns.DATE_MODIFIED} >= ?)" to arrayOf(from, from)
            }
            else -> null to null
        }
        return resolver.query(kind.uriFor(volume), projection, selection, args, null)?.use { cursor ->
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
            // MediaStore leaves DATE_TAKEN empty when the EXIF date has no offset, and for files
            // without EXIF (screenshots, downloads): the file's time is then the phone's best guess.
            // The server prefers the photo's own EXIF date whenever it has one.
            takenAt = takenAt(if (c.isNull(4) || c.getLong(4) <= 0) modifiedSeconds * MILLIS else c.getLong(4)),
            album = c.getString(6), // BUCKET_ID: the filter's key (names repeat across volumes)
        )
        return Row(asset, if (generations) c.getLong(8) else maxOf(c.getLong(7), modifiedSeconds))
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

}

/** DATE_TAKEN (epoch ms) as RFC 3339 with the phone's offset at that instant; null when unknown. */
fun takenAt(epochMillis: Long, zone: ZoneId = ZoneId.systemDefault()): String? {
    if (epochMillis <= 0) return null
    return Instant.ofEpochMilli(epochMillis).atZone(zone).toOffsetDateTime()
        .truncatedTo(ChronoUnit.SECONDS)
        .format(DateTimeFormatter.ISO_OFFSET_DATE_TIME)
}

private const val API29_VERSION = "dm"
/** Bound parameters per IN (…) query (SQLite's limit is 999). */
private const val EXISTING_CHUNK = 500

/** Where a volume's last scan stopped: its MediaStore version and generation (API 29: seconds). */
data class VolumeCursor(val version: String, val position: Long)

/** `volume=version|position;…`; anything unreadable means "scan everything again". */
fun encodeCursors(cursors: Map<String, VolumeCursor>): String =
    cursors.entries.sortedBy { it.key }.joinToString(";") { (volume, c) -> "$volume=${c.version}|${c.position}" }

fun decodeCursors(token: String?): Map<String, VolumeCursor> {
    if (token.isNullOrBlank() || '=' !in token) return emptyMap()
    return token.split(';').mapNotNull { part ->
        val volume = part.substringBefore('=', "").takeIf { it.isNotEmpty() } ?: return@mapNotNull null
        val value = part.substringAfter('=')
        val position = value.substringAfterLast('|').toLongOrNull() ?: return@mapNotNull null
        volume to VolumeCursor(value.substringBeforeLast('|'), position)
    }.toMap()
}

/** The next cursor: the generation snapshot (API 30+), else the newest time seen (API 29). */
fun nextPosition(since: Long?, snapshot: Long?, seen: List<Long>): Long =
    snapshot ?: maxOf(since ?: 0L, seen.maxOrNull() ?: 0L)
