package app.squirrel.shared.ios.transfer

import app.squirrel.shared.api.ApiError
import app.squirrel.shared.sync.AssetReader
import app.squirrel.shared.sync.ChunkTransport
import app.squirrel.shared.sync.UploadSession
import app.squirrel.shared.sync.UploadTokenRefused
import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.addressOf
import kotlinx.cinterop.usePinned
import kotlinx.io.IOException
import platform.Foundation.NSData
import platform.Foundation.NSMutableURLRequest
import platform.Foundation.NSURL
import platform.Foundation.NSURLSessionConfiguration
import platform.Foundation.create
import platform.Foundation.setHTTPMethod
import platform.Foundation.setValue
import platform.Foundation.writeToFile

private const val UNAUTHORIZED = 401

/** The background session's configuration: launches the app for its events, never waits for Wi-Fi itself. */
fun backgroundConfiguration(): NSURLSessionConfiguration =
    NSURLSessionConfiguration.backgroundSessionConfigurationWithIdentifier(UPLOAD_SESSION_ID).apply {
        sessionSendsLaunchEvents = true
        discretionary = false
    }

/**
 * Chunks through [transfers] with the upload's own token (`Authorization: Upload squ_…`), so a
 * transfer the system finishes in the background does not depend on the 15-minute access token.
 */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class BackgroundTransport(
    private val baseUrl: String,
    private val transfers: TransferSession,
    /** "Only on Wi-Fi": no cellular or Low Data Mode transfers. */
    private val wifiOnly: () -> Boolean = { false },
) : ChunkTransport {
    override val needsUploadToken: Boolean = true

    override suspend fun put(session: UploadSession, index: Int, reader: AssetReader) {
        val token = session.uploadToken ?: throw UploadTokenRefused()
        val offset = index * session.chunkSize
        val length = minOf(session.chunkSize, reader.size - offset).toInt()
        val request = NSMutableURLRequest(NSURL(string = "$baseUrl/api/uploads/${session.uploadId}/chunks/$index")).apply {
            setHTTPMethod("PUT")
            setValue("Upload $token", forHTTPHeaderField = "Authorization")
            setValue("application/octet-stream", forHTTPHeaderField = "Content-Type")
            setValue(length.toString(), forHTTPHeaderField = "Content-Length")
            setAllowsExpensiveNetworkAccess(!wifiOnly())
            setAllowsConstrainedNetworkAccess(!wifiOnly())
        }
        val result = transfers.send("${session.uploadId}:$index", request) { path -> write(reader, offset, length, path) }
        when {
            result.succeeded -> Unit
            result.status == UNAUTHORIZED -> throw UploadTokenRefused()
            result.status == 0 -> throw IOException(result.error ?: "The chunk upload failed")
            else -> throw ApiError(result.status, "http_${result.status}", "Chunk $index refused (HTTP ${result.status})", fromServer = false)
        }
    }

    private suspend fun write(reader: AssetReader, offset: Long, length: Int, path: String) {
        val bytes = ByteArray(length)
        var filled = 0
        while (filled < length) {
            val read = reader.read(offset + filled, bytes, filled, length - filled)
            if (read < 0) break
            filled += read
        }
        if (filled != length) throw IOException("The file changed while it was read")
        val data = bytes.usePinned { NSData.create(bytes = it.addressOf(0), length = length.toULong()) }
        if (!data.writeToFile(path, atomically = true)) throw IOException("Could not write the chunk file")
    }
}
