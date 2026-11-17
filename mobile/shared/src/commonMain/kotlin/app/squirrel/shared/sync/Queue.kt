package app.squirrel.shared.sync

import app.cash.sqldelight.coroutines.asFlow
import app.cash.sqldelight.coroutines.mapToList
import app.squirrel.shared.db.Local_asset
import app.squirrel.shared.db.SquirrelDatabase
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

enum class QueueState(val sql: String) {
    New("new"),
    Hashed("hashed"),
    Missing("missing"),
    Uploading("uploading"),
    Done("done"),
    Skipped("skipped"),
    Failed("failed"),
    ;

    companion object {
        fun of(sql: String): QueueState = entries.first { it.sql == sql }
    }
}

/** A queued phone asset. */
data class QueuedAsset(
    val deviceAssetId: String,
    val kind: String,
    val name: String,
    val mime: String?,
    val size: Long,
    val modifiedAt: Long,
    val takenAt: String?,
    val album: String?,
    val sha256: String?,
    val state: QueueState,
    val uploadId: String?,
    val attempts: Int,
    val nextAttemptAt: Long,
    val lastError: String?,
    val hashedSize: Long? = null,
    val hashedModifiedAt: Long? = null,
    /** Read without its GPS: the server's copy has no location. */
    val locationStripped: Boolean = false,
)

private fun Local_asset.toQueued() = QueuedAsset(
    deviceAssetId = device_asset_id,
    kind = kind,
    name = name,
    mime = mime,
    size = size,
    modifiedAt = modified_at,
    takenAt = taken_at,
    album = album,
    sha256 = sha256,
    state = QueueState.of(state),
    uploadId = upload_id,
    attempts = attempts.toInt(),
    nextAttemptAt = next_attempt_at,
    lastError = last_error,
    hashedSize = hashed_size,
    hashedModifiedAt = hashed_modified_at,
    locationStripped = location_stripped != 0L,
)

/** The backup queue in `local_asset`: what is known, hashed, missing, uploaded. */
class Queue(private val db: SquirrelDatabase) {
    private val q = db.localAssetQueries

    fun get(id: String): QueuedAsset? = q.byId(id).executeAsOneOrNull()?.toQueued()

    fun nextToHash(limit: Long): List<QueuedAsset> = q.nextToHash(limit).executeAsList().map { it.toQueued() }

    /** Records the hash of what was read: [size] and [modifiedAt] as hashed (null: the scanned values). */
    fun setHash(id: String, sha256: String, size: Long? = null, modifiedAt: Long? = null, locationStripped: Boolean = false) {
        q.setHash(sha256, size, modifiedAt, if (locationStripped) 1L else 0L, id)
    }

    fun hashedBatch(limit: Long): List<QueuedAsset> = q.hashedBatch(limit).executeAsList().map { it.toQueued() }

    /** Applies a sync check: [missing] will be uploaded, the rest of [checked] is already backed up. */
    fun applyCheck(checked: List<String>, missing: Collection<String>) = db.transaction {
        val missingSet = missing.toSet()
        val (toUpload, present) = checked.partition { it in missingSet }
        toUpload.chunked(SQL_IN_LIMIT).forEach { q.markMissing(it) }
        present.chunked(SQL_IN_LIMIT).forEach { q.markPresent(it) }
    }

    fun markPresent(ids: List<String>) = ids.chunked(SQL_IN_LIMIT).forEach { q.markPresent(it) }

    fun nextToUpload(now: Long): QueuedAsset? = q.nextToUpload(now).executeAsOneOrNull()?.toQueued()

    /** The ids due for upload at [now], in upload order (resumed sessions first). */
    fun uploadCandidates(now: Long): List<String> = q.uploadCandidates(now).executeAsList()

    fun markUploading(id: String, uploadId: String) {
        q.markUploading(uploadId, id)
    }

    fun markDone(id: String) {
        q.markDone(id)
    }

    fun recordAttempt(id: String, error: String, nextAttemptAt: Long) {
        q.recordAttempt(error, nextAttemptAt, id)
    }

    fun markFailed(id: String, error: String) {
        q.markFailed(error, id)
    }

    fun markSkipped(id: String) {
        q.markSkipped(id)
    }

    /** The file changed under its hash: hash and check it again. */
    fun resetHash(id: String) {
        q.resetHash(id)
    }

    /** The server forgot the upload session: start a new one. */
    fun forgetUpload(id: String) {
        q.forgetUpload(id)
    }

    /** "Back up now": failed assets get another round. */
    fun retryFailed() {
        q.retryFailed()
    }

    fun counts(): Map<QueueState, Long> = q.counts().executeAsList().associate { QueueState.of(it.state) to it.n }

    fun countsFlow(): Flow<Map<QueueState, Long>> =
        q.counts().asFlow().mapToList(Dispatchers.IO).map { rows -> rows.associate { QueueState.of(it.state) to it.n } }

    fun bytesLeft(): Long = q.bytesLeft().executeAsOne()

    fun bytesLeftFlow(): Flow<Long> =
        q.bytesLeft().asFlow().map { it.executeAsOne() }

    fun failures(): List<QueuedAsset> = q.failures().executeAsList().map { it.toQueued() }

    private companion object {
        /** SQLite's default bound-parameter limit is 999. */
        const val SQL_IN_LIMIT = 500
    }
}
