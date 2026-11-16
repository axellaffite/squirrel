package app.squirrel.shared.sync

import app.squirrel.api.models.UploadSessionDto
import app.squirrel.shared.api.ChunkAuth
import app.squirrel.shared.api.ChunkUploader

/** An upload session as the uploader tracks it. `chunkSize` comes from the server and is authoritative. */
data class UploadSession(val uploadId: String, val chunkSize: Long, val chunkCount: Int, val received: Set<Int>) {
    val missing: List<Int> get() = (0 until chunkCount).filterNot { it in received }

    companion object {
        fun of(dto: UploadSessionDto) = UploadSession(dto.uploadId, dto.chunkSize, dto.chunkCount, dto.receivedChunks.toSet())
    }
}

/**
 * Sends one chunk; throws an `ApiError` or a network exception on failure.
 * Ktor on Android; background URLSession on iOS (Milestone H).
 */
interface ChunkTransport {
    suspend fun put(session: UploadSession, index: Int, reader: AssetReader)
}

/** Chunks through Ktor with the device's bearer token. */
class KtorChunkTransport(private val chunks: ChunkUploader) : ChunkTransport {
    override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) =
        chunks.put(session.uploadId, index, session.chunkSize, reader, ChunkAuth.Bearer)
}
