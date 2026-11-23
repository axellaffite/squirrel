package app.squirrel.shared.sync

import app.squirrel.api.models.NodeDto
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.NewUpload
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.UploadOutcome
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.hash.hashAsset
import kotlinx.coroutines.delay

/** Hashing is this share of an upload's progress; the chunks are the rest. */
private const val HASH_SHARE = 0.1f
private const val COMPLETE_TRIES = 3
private const val COMPLETE_RETRY_MS = 1_000L
private const val SERVICE_UNAVAILABLE = 503

/**
 * Uploads one file the user picked into a folder, with the backup engine's pieces: the same
 * hashing, `POST /api/uploads` (a file the caller already has is linked, nothing sent), the
 * missing chunks through a [ChunkTransport], then complete (tried again while the server is
 * busy). No device asset id: a file put into a folder is not a phone backup.
 */
class FileUpload(
    private val api: SquirrelApi,
    private val transport: ChunkTransport = KtorChunkTransport(api.chunks),
    private val completeRetryMs: Long = COMPLETE_RETRY_MS,
) {
    /** The new file; throws an [ApiError] (quota, refused, gone…) or a network exception. */
    suspend fun upload(parentId: String, name: String, reader: AssetReader, onProgress: (Float) -> Unit = {}): NodeDto {
        val size = reader.size
        val sha = hashAsset(reader) { done -> onProgress(if (size == 0L) HASH_SHARE else HASH_SHARE * done / size) }
        val outcome = api.uploads.create(NewUpload(parentId, name, size, sha, takenAt = null, deviceAssetId = null))
        val session = when (outcome) {
            is UploadOutcome.Deduplicated -> return outcome.node.also { onProgress(1f) }
            is UploadOutcome.Session -> UploadSession.of(outcome.session)
        }
        val missing = session.missing
        missing.forEachIndexed { n, index ->
            transport.put(session, index, reader)
            onProgress(HASH_SHARE + (1 - HASH_SHARE) * (n + 1) / missing.size)
        }
        return complete(session.uploadId).also { onProgress(1f) }
    }

    private suspend fun complete(uploadId: String): NodeDto {
        var tries = 0
        while (true) {
            try {
                return api.uploadsApi.completeUpload(uploadId).bodyOrThrow()
            } catch (e: ApiError) {
                tries += 1
                if (e.status != SERVICE_UNAVAILABLE || tries >= COMPLETE_TRIES) throw e
                delay(e.retryAfterSeconds?.let { it * 1000 } ?: completeRetryMs)
            }
        }
    }
}
