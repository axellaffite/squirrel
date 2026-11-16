package app.squirrel.shared.api

import app.squirrel.shared.sync.AssetReader
import io.ktor.client.HttpClient
import io.ktor.client.plugins.timeout
import io.ktor.client.request.header
import io.ktor.client.request.put
import io.ktor.client.request.setBody
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.content.OutgoingContent
import io.ktor.utils.io.ByteWriteChannel
import io.ktor.utils.io.writeFully

/** How a chunk PUT authenticates. */
sealed interface ChunkAuth {
    /** The device's access token (refreshed on 401). */
    data object Bearer : ChunkAuth

    /** `Authorization: Upload <token>`, scoped to one upload (iOS background transfers, Milestone H). */
    data class UploadToken(val token: String) : ChunkAuth
}

/** The server's `CHUNK_TIMEOUT`: one 8 MiB chunk on a slow uplink. */
internal const val CHUNK_TIMEOUT_MS = 5 * 60_000L
private const val COPY_BUFFER = 64 * 1024

/**
 * `PUT /api/uploads/{id}/chunks/{index}`, hand-written: the generated method takes the body as a
 * JSON value and knows only the bearer scheme. This one streams the slice straight from the
 * [AssetReader] with its exact `Content-Length`.
 */
class ChunkUploader internal constructor(
    private val baseUrl: String,
    private val authed: HttpClient,
    private val public: HttpClient,
) {
    suspend fun put(uploadId: String, index: Int, chunkSize: Long, reader: AssetReader, auth: ChunkAuth) {
        val start = index * chunkSize
        require(start < reader.size) { "Chunk $index starts past the end of the file" }
        val length = minOf(chunkSize, reader.size - start)
        val client = if (auth is ChunkAuth.Bearer) authed else public
        client.put("$baseUrl/api/uploads/$uploadId/chunks/$index") {
            timeout { requestTimeoutMillis = CHUNK_TIMEOUT_MS }
            if (auth is ChunkAuth.UploadToken) header(HttpHeaders.Authorization, "Upload ${auth.token}")
            setBody(SliceContent(reader, start, length))
        }.throwUnlessSuccess()
    }
}

/** A slice of an asset as a request body; re-readable, so a retry after a refresh resends it. */
private class SliceContent(
    private val reader: AssetReader,
    private val start: Long,
    private val length: Long,
) : OutgoingContent.WriteChannelContent() {
    override val contentType: ContentType = ContentType.Application.OctetStream
    override val contentLength: Long = length

    override suspend fun writeTo(channel: ByteWriteChannel) {
        val buffer = ByteArray(COPY_BUFFER)
        var sent = 0L
        while (sent < length) {
            val want = minOf(COPY_BUFFER.toLong(), length - sent).toInt()
            val n = reader.read(start + sent, buffer, 0, want)
            check(n > 0) { "The file ended before its recorded size" }
            channel.writeFully(buffer, 0, n)
            sent += n
        }
    }
}
