package app.squirrel.shared.sync

import app.squirrel.api.models.SyncFolderRequest
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.NewUpload
import app.squirrel.shared.api.SignedOut
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.UploadOutcome
import app.squirrel.shared.api.bodyOrThrow
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlin.concurrent.atomics.AtomicInt
import kotlin.concurrent.atomics.ExperimentalAtomicApi
import kotlin.concurrent.atomics.incrementAndFetch
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.io.IOException
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
    /** The server could not be reached at all (only with `reportOffline`): no attempt is spent. */
    data object Offline : UploadResult
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
private const val TOO_MANY_REQUESTS = 429
private const val SERVICE_UNAVAILABLE = 503
private const val MILLIS_PER_SECOND = 1000L

/** Refusals that will not change by retrying the same file. */
private val PERMANENT = setOf(400, 403, PAYLOAD_TOO_LARGE, 422)

/**
 * Uploads one queued asset: the year folder, `POST /api/uploads`, the chunks the server lacks
 * ([parallelChunks] at a time), then complete. Resumes a recorded session. One instance per run
 * (it caches the year folders), safe to share between assets uploading at the same time.
 * [onServerBusy] hears a 429 or 503 with the time this asset waits until, so others can wait too;
 * with [reportOffline], an unreachable server is [UploadResult.Offline] instead of an attempt.
 */
@OptIn(ExperimentalAtomicApi::class)
class Uploader(
    private val api: SquirrelApi,
    private val transport: ChunkTransport,
    private val source: AssetSource,
    private val queue: Queue,
    private val now: () -> Long,
    private val random: () -> Double = { Random.nextDouble() },
    private val parallelChunks: Int = 2,
    private val timeZone: TimeZone = TimeZone.currentSystemDefault(),
    private val pollMillis: Long = 1_000,
    private val stopGraceMillis: Long = 10_000,
    private val reportOffline: Boolean = false,
    private val onServerBusy: (until: Long) -> Unit = {},
) {
    private val folders = mutableMapOf<Int, String>()
    private val foldersLock = Mutex()

    private sealed interface Transfer {
        data object Done : Transfer
        data object Interrupted : Transfer
        data object Restart : Transfer

        /** The session lives but its token was refused: open it again for a fresh token. */
        data object Reopen : Transfer
    }

    suspend fun upload(
        asset: QueuedAsset,
        shouldContinue: () -> Boolean = { true },
        onProgress: (Float) -> Unit = {},
    ): UploadResult {
        val reader = source.open(asset.deviceAssetId) ?: run {
            queue.markSkipped(asset.deviceAssetId)
            return UploadResult.Skipped
        }
        val result = reader.use {
            try {
                transfer(asset, reader, shouldContinue, onProgress)
            } catch (e: CancellationException) {
                throw e
            } catch (e: SignedOut) {
                throw e
            } catch (e: RunStopped) {
                throw e
            } catch (e: ApiError) {
                handleApiError(asset, e)
            } catch (e: IOException) {
                if (reportOffline) UploadResult.Offline else retry(asset, e)
            } catch (e: Exception) {
                retry(asset, e)
            }
        }
        if (result == UploadResult.Uploaded || result == UploadResult.Deduplicated) source.backedUp(asset.deviceAssetId)
        return result
    }

    private suspend fun transfer(
        asset: QueuedAsset,
        reader: AssetReader,
        shouldContinue: () -> Boolean,
        onProgress: (Float) -> Unit,
    ): UploadResult {
        if (changedSinceHashing(asset, reader)) {
            queue.resetHash(asset.deviceAssetId)
            return UploadResult.Changed
        }
        var uploadId = asset.uploadId
        repeat(MAX_RESTARTS) {
            // A token-bound transport resumes by opening the session again (the status call gives no token).
            val session = uploadId?.takeUnless { transport.needsUploadToken }?.let { status(it) } ?: when (val created = create(asset)) {
                is UploadOutcome.Deduplicated -> {
                    queue.markDone(asset.deviceAssetId)
                    return UploadResult.Deduplicated
                }
                is UploadOutcome.Session -> UploadSession.of(created.session).also {
                    queue.markUploading(asset.deviceAssetId, it.uploadId)
                }
            }
            when (sendAndComplete(session, reader, shouldContinue, onProgress)) {
                Transfer.Done -> {
                    queue.markDone(asset.deviceAssetId)
                    return UploadResult.Uploaded
                }
                Transfer.Interrupted -> return UploadResult.Interrupted
                Transfer.Restart -> {
                    queue.forgetUpload(asset.deviceAssetId)
                    uploadId = null
                }
                Transfer.Reopen -> Unit // the next pass resumes the same session with a new token
            }
        }
        throw ApiError(NOT_FOUND, "not_found", "The server kept losing the upload session")
    }

    private suspend fun sendAndComplete(
        start: UploadSession,
        reader: AssetReader,
        shouldContinue: () -> Boolean,
        onProgress: (Float) -> Unit,
    ): Transfer {
        var session = start
        repeat(MAX_COMPLETE_TRIES) {
            try {
                if (!send(session, reader, shouldContinue, onProgress)) return Transfer.Interrupted
                api.uploadsApi.completeUpload(session.uploadId).bodyOrThrow()
                return Transfer.Done
            } catch (_: UploadTokenRefused) {
                // The token may only have been replaced: the device's bearer tells whether the
                // session is gone (start over) or alive (reopen it, keeping what the server has).
                return if (status(session.uploadId) == null) Transfer.Restart else Transfer.Reopen
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

    /**
     * Sends the missing chunks, [parallelChunks] at a time; false when [shouldContinue] turned
     * false. It is checked before each chunk and every [pollMillis] while chunks are in flight;
     * those get [stopGraceMillis] to finish, then are cancelled (the server keeps what arrived).
     */
    private suspend fun send(
        session: UploadSession,
        reader: AssetReader,
        shouldContinue: () -> Boolean,
        onProgress: (Float) -> Unit,
    ): Boolean =
        coroutineScope {
            val gate = Semaphore(parallelChunks)
            val stored = AtomicInt(session.received.size)
            onProgress(stored.load().toFloat() / session.chunkCount)
            val chunks = mutableListOf<Job>()
            val producer = launch {
                for (index in session.missing) {
                    gate.acquire()
                    if (!shouldContinue()) {
                        gate.release()
                        return@launch
                    }
                    chunks += this@coroutineScope.launch {
                        try {
                            transport.put(session, index, reader)
                            onProgress(stored.incrementAndFetch().toFloat() / session.chunkCount)
                        } finally {
                            gate.release()
                        }
                    }
                }
            }
            while (producer.isActive || chunks.any { it.isActive }) {
                if (!shouldContinue()) {
                    producer.cancel()
                    withTimeoutOrNull(stopGraceMillis) { chunks.toList().joinAll() }
                    chunks.forEach { it.cancel() }
                    return@coroutineScope false
                }
                withTimeoutOrNull(pollMillis) {
                    producer.join()
                    chunks.toList().joinAll()
                }
            }
            session.missing.size == chunks.size
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

    private suspend fun folder(year: Int): String = foldersLock.withLock {
        folders[year] ?: api.sync.syncFolder(SyncFolderRequest(year)).bodyOrThrow().folderId.also { folders[year] = it }
    }

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
        else -> retry(asset, e, minDelay = e.retryAfterSeconds?.times(MILLIS_PER_SECOND)).also { result ->
            if (result is UploadResult.RetryLater && (e.status == TOO_MANY_REQUESTS || e.status == SERVICE_UNAVAILABLE)) {
                onServerBusy(result.at)
            }
        }
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
