package app.squirrel.shared.sync

import app.squirrel.shared.db.SquirrelDatabase
import app.squirrel.shared.db.recordScanned
import app.squirrel.shared.settings.SettingsStore

data class ScanSummary(val seen: Int, val removed: Int, val full: Boolean)

private const val SOURCE_KEY = "phone"
private const val FILTER_KEY = "scan.filter"

/**
 * Brings the queue up to date with the phone. Incremental from the stored token; a full rescan
 * (first run, or the album/video selection changed) skips every queued asset it no longer sees.
 * Nothing is ever deleted from the server.
 */
class Scanner(
    private val source: AssetSource,
    private val db: SquirrelDatabase,
    private val queue: Queue,
    private val settings: SettingsStore,
    private val now: () -> Long = { 0L },
) {
    suspend fun scan(): ScanSummary {
        val backup = settings.backup()
        val filter = SourceFilter(backup.includeVideos, backup.albums)
        val stored = db.scanStateQueries.get(SOURCE_KEY).executeAsOneOrNull()
        val sameFilter = settings.value(FILTER_KEY) == filter.fingerprint()
        val token = stored?.token?.takeIf { sameFilter }
        val result = source.changes(token, filter)
        val full = result.full || token == null
        val generation = (stored?.generation ?: 0L) + if (full) 1 else 0
        val (accepted, rejected) = result.assets.partition(filter::accepts)

        db.transaction {
            accepted.forEach { db.recordScanned(it, generation) }
            if (full) {
                db.localAssetQueries.skipUnseen(generation)
            } else {
                (result.removed + rejected.map { it.deviceAssetId }).chunked(SKIP_CHUNK)
                    .forEach { db.localAssetQueries.skip(it) }
            }
            db.scanStateQueries.put(SOURCE_KEY, result.token, generation, now())
            settings.setValue(FILTER_KEY, filter.fingerprint())
        }
        return ScanSummary(seen = accepted.size, removed = if (full) 0 else result.removed.size, full = full)
    }

    private companion object {
        const val SKIP_CHUNK = 500
    }
}
