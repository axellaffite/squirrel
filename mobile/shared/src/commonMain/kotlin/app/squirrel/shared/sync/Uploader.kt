package app.squirrel.shared.sync

import app.squirrel.api.models.SyncFolderRequest
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.NewUpload
import app.squirrel.shared.api.SignedOut
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.UploadOutcome
import app.squirrel.shared.api.bodyOrThrow
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.datetime.TimeZone
import kotlinx.datetime.toLocalDateTime
import kotlin.random.Random
import kotlin.time.Instant

sealed interface UploadResult {
    data object Uploaded : UploadResult
    data object Deduplicated : UploadResult
    /** Gone from the phone. */
    data object Skipped : UploadResult
    /** The bytes no longer match the hash: back to hashing. */
    data object Changed : UploadResult
    /** Stopped between chunks because conditions or the deadline said so; the session is kept. */
    data object Interrupted : UploadResult
    data class RetryLater(val at: Long) : UploadResult
    /** Out of attempts; retried on the next "Back up now". */
    data object Failed : UploadResult
}

enum class StopReason { QuotaExceeded, StorageFull }

/** Ends the whole run: nothing else can be uploaded until the user acts. */
class RunStopped(val reason: StopReason) : Exception("Upload run stopped: $reason")

const val MAX_ATTEMPTS = 8
private const val MAX_RESTARTS = 3
private const val MAX_COMPLETE_TRIES = 3
private const val NOT_FOUND = 404
private const val INSUFFICIENT_STORAGE = 507
private const val CONFLICT = 409
private const val PAYLOAD_TOO_LARGE = 413
private const val MILLIS_PER_SECOND = 1000L

/** Refusals that will not change by retrying the same file. */
private val PERMANENT = setOf(400, 403, PAYLOAD_TOO_LARGE, 422)

/**
 * Uploads one queued asset: the year folder, `POST /api/uploads`, the chunks the server lacks
 * ([parallelChunks] at a time), then complete. Resumes a recorded session. One instance per run
 * (it caches the year folders).
 */
class Uploader(
    private val api: SquirrelApi,
    private val transport: ChunkTransport,
    private val source: AssetSource,
    private val queue: Queue,
    private val now: () -> Long,
    private val random: () -> Double = { Random.nextDouble() },
    private val parallelChunks: Int = 2,
    private val timeZone: TimeZone = TimeZone.currentSystemDefault(),
) {
    private val folders = mutableMapOf<Int, String>()

    private sealed interface Transfer {
        data object Done : Transfer
        data object Interrupted : Transfer
        data object Restart : Transfer
    }

    suspend fun upload(asset: QueuedAsset, shouldContinue: () -> Boolean = { true }): UploadResult {
        val reader = source.open(asset.deviceAssetId) ?: run {
            queue.markSkipped(asset.deviceAssetId)
            return UploadResult.Skipped
        }
        return reader.use {
            try {
                transfer(asset, reader, shouldContinue)
            } catch (e: CancellationException) {
                throw e
            } catch (e: SignedOut) {
                throw e
            } catch (e: RunStopped) {
                throw e
            } catch (e: ApiError) {
                handleApiError(asset, e)
            } catch (e: Exception) {
                retry(asset, e)
            }
        }
    }

    private suspend fun transfer(asset: QueuedAsset, reader: AssetReader, shouldContinue: () -> Boolean): UploadResult {
        if (changedSinceHashing(asset, reader)) {
            queue.resetHash(asset.deviceAssetId)
            return UploadResult.Changed
        }
        var uploadId = asset.uploadId
        repeat(MAX_RESTARTS) {
            val session = uploadId?.let { status(it) } ?: when (val created = create(asset)) {
                is UploadOutcome.Deduplicated -> {
                    queue.markDone(asset.deviceAssetId)
                    return UploadResult.Deduplicated
                }
                is UploadOutcome.Session -> UploadSession.of(created.session).also {
                    queue.markUploading(asset.deviceAssetId, it.uploadId)
                }
            }
            when (sendAndComplete(session, reader, shouldContinue)) {
                Transfer.Done -> {
                    queue.markDone(asset.deviceAssetId)
                    return UploadResult.Uploaded
                }
                Transfer.Interrupted -> return UploadResult.Interrupted
                Transfer.Restart -> {
                    queue.forgetUpload(asset.deviceAssetId)
                    uploadId = null
                }
            }
        }
        throw ApiError(NOT_FOUND, "not_found", "The server kept losing the upload session")
    }

    private suspend fun sendAndComplete(start: UploadSession, reader: AssetReader, shouldContinue: () -> Boolean): Transfer {
        var session = start
        repeat(MAX_COMPLETE_TRIES) {
            try {
                if (!send(session, reader, shouldContinue)) return Transfer.Interrupted
                api.uploadsApi.completeUpload(session.uploadId).bodyOrThrow()
                return Transfer.Done
            } catch (e: ApiError) {
                when {
                    e.status == NOT_FOUND -> return Transfer.Restart
                    e.code == "upload_incomplete" -> session = status(session.uploadId) ?: return Transfer.Restart
                    else -> throw e
                }
            }
        }
        throw ApiError(409, "upload_incomplete", "The server kept missing chunks")
    }

    /** Sends the missing chunks; false when [shouldContinue] stopped it between chunks. */
    private suspend fun send(session: UploadSession, reader: AssetReader, shouldContinue: () -> Boolean): Boolean {
        val gate = Semaphore(parallelChunks)
        var completed = true
        coroutineScope {
            for (index in session.missing) {
                gate.acquire()
                if (!shouldContinue()) {
                    gate.release()
                    completed = false
                    break
                }
                launch {
                    try {
                        transport.put(session, index, reader)
                    } finally {
                        gate.release()
                    }
                }
            }
        }
        return completed
    }

    /** The file on the phone is no longer the one that was hashed (edited between hash and upload). */
    private fun changedSinceHashing(asset: QueuedAsset, reader: AssetReader): Boolean {
        val hashedSize = asset.hashedSize ?: asset.size
        val hashedModified = asset.hashedModifiedAt
        val modified = reader.modifiedAt
        return reader.size != hashedSize || (modified != null && hashedModified != null && modified != hashedModified)
    }

    private suspend fun status(uploadId: String): UploadSession? {
        val response = api.uploadsApi.uploadStatus(uploadId)
        if (response.status == NOT_FOUND) return null
        return UploadSession.of(response.bodyOrThrow())
    }

    private suspend fun create(asset: QueuedAsset): UploadOutcome = api.uploads.create(
        NewUpload(
            parentId = folder(year(asset)),
            name = asset.name,
            size = asset.hashedSize ?: asset.size,
            sha256 = checkNotNull(asset.sha256) { "Upload before hashing" },
            takenAt = asset.takenAt,
            deviceAssetId = asset.deviceAssetId,
        ),
    )

    private suspend fun folder(year: Int): String = folders[year]
        ?: api.sync.syncFolder(SyncFolderRequest(year)).bodyOrThrow().folderId.also { folders[year] = it }

    /** The capture year on the phone's clock: from `taken_at`, else the file's modification time. */
    private fun year(asset: QueuedAsset): Int =
        asset.takenAt?.take(4)?.toIntOrNull()
            ?: Instant.fromEpochMilliseconds(asset.modifiedAt).toLocalDateTime(timeZone).year

    private fun handleApiError(asset: QueuedAsset, e: ApiError): UploadResult = when {
        e.code == "quota_exceeded" -> throw RunStopped(StopReason.QuotaExceeded)
        e.code == "storage_full" || e.status == INSUFFICIENT_STORAGE -> throw RunStopped(StopReason.StorageFull)
        e.code == "hash_mismatch" -> {
            queue.resetHash(asset.deviceAssetId)
            UploadResult.Changed
        }
        e.status in PERMANENT || (e.status == CONFLICT && e.code != "upload_incomplete") -> {
            queue.recordAttempt(asset.deviceAssetId, refusal(e), now())
            queue.markFailed(asset.deviceAssetId, refusal(e))
            UploadResult.Failed
        }
        else -> retry(asset, e, minDelay = e.retryAfterSeconds?.times(MILLIS_PER_SECOND))
    }

    /** A message the Backup screen can show for a file the server will not take. */
    private fun refusal(e: ApiError): String = when {
        e.status == PAYLOAD_TOO_LARGE -> "This file is larger than your Squirrel accepts"
        e.fromServer && !e.message.isNullOrBlank() -> e.message.orEmpty()
        else -> "Your Squirrel refused this file (HTTP ${e.status})"
    }

    private fun retry(asset: QueuedAsset, e: Exception, minDelay: Long? = null): UploadResult {
        val attempts = (queue.get(asset.deviceAssetId)?.attempts ?: asset.attempts) + 1
        val message = e.message ?: e::class.simpleName ?: "Upload failed"
        if (attempts >= MAX_ATTEMPTS) {
            queue.recordAttempt(asset.deviceAssetId, message, now())
            queue.markFailed(asset.deviceAssetId, message)
            return UploadResult.Failed
        }
        val at = now() + maxOf(backoffMillis(attempts - 1, random), minDelay ?: 0L)
        queue.recordAttempt(asset.deviceAssetId, message, at)
        return UploadResult.RetryLater(at)
    }
}
