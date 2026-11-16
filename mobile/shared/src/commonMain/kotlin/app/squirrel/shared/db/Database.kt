package app.squirrel.shared.db

import app.cash.sqldelight.db.SqlDriver

/** Opens the app's one database (`squirrel.db`) over a platform driver. */
fun createDatabase(driver: SqlDriver): SquirrelDatabase = SquirrelDatabase(driver)

/** One asset as a scan reports it. */
data class ScannedAsset(
    val deviceAssetId: String,
    val kind: String,
    val name: String,
    val mime: String?,
    val size: Long,
    val modifiedAt: Long,
    /** RFC 3339 with the phone's UTC offset. */
    val takenAt: String?,
    val album: String?,
)

/**
 * Inserts or refreshes a scanned asset. An unchanged file (same size and modification time)
 * keeps its hash and state; a changed one goes back to `new`.
 */
fun SquirrelDatabase.recordScanned(asset: ScannedAsset, generation: Long) = transaction {
    with(asset) {
        localAssetQueries.insertScanned(deviceAssetId, kind, name, mime, size, modifiedAt, takenAt, album, generation)
        localAssetQueries.updateScanned(kind, name, mime, album, takenAt, generation, size, modifiedAt, deviceAssetId)
    }
}

/** Number of assets per queue state. */
fun SquirrelDatabase.counts(): Map<String, Long> =
    localAssetQueries.counts().executeAsList().associate { it.state to it.n }
