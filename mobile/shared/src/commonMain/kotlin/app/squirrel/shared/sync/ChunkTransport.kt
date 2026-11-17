package app.squirrel.shared.sync

import app.squirrel.api.models.UploadSessionDto
import app.squirrel.shared.api.ChunkAuth
import app.squirrel.shared.api.ChunkUploader

/** An upload session as the uploader tracks it. `chunkSize` comes from the server and is authoritative. */
data class UploadSession(
    val uploadId: String,
    val chunkSize: Long,
    val chunkCount: Int,
    val received: Set<Int>,
    /** `squ_…`: this upload's chunks only, given to devices when the session is opened or resumed. */
    val uploadToken: String? = null,
) {
    val missing: List<Int> get() = (0 until chunkCount).filterNot { it in received }

    override fun toString(): String = "UploadSession($uploadId, $chunkCount chunks of $chunkSize, ${received.size} received)"

    companion object {
        fun of(dto: UploadSessionDto) =
            UploadSession(dto.uploadId, dto.chunkSize, dto.chunkCount, dto.receivedChunks.toSet(), dto.uploadToken)
    }
}

/** The server refused this upload's token (replaced by a newer one): open the session again. */
class UploadTokenRefused : Exception("The upload token was refused")

/**
 * Sends one chunk; throws an `ApiError` or a network exception on failure.
 * Ktor on Android; background URLSession on iOS (Milestone H).
 */
interface ChunkTransport {
    @Throws(Exception::class)
    suspend fun put(session: UploadSession, index: Int, reader: AssetReader)

    /**
     * Chunks go with the upload's own token (iOS background transfers outlive the access token):
     * a recorded session is resumed through `POST /api/uploads`, which issues a fresh token.
     */
    val needsUploadToken: Boolean get() = false
}

/** Chunks through Ktor with the device's bearer token. */
class KtorChunkTransport(private val chunks: ChunkUploader) : ChunkTransport {
    override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) =
        chunks.put(session.uploadId, index, session.chunkSize, reader, ChunkAuth.Bearer)
}
