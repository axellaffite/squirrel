package app.squirrel.android.media

import android.os.ParcelFileDescriptor
import app.squirrel.shared.sync.AssetReader
import java.io.FileInputStream
import java.nio.ByteBuffer

/** Positional reads on a MediaStore file descriptor (chunks must be re-readable to resume). */
class MediaStoreReader(
    private val descriptor: ParcelFileDescriptor,
    override val size: Long,
    override val modifiedAt: Long?,
    override val locationStripped: Boolean = false,
) : AssetReader {
    private val stream = FileInputStream(descriptor.fileDescriptor)
    private val channel = stream.channel

    override suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int =
        channel.read(ByteBuffer.wrap(into, offset, length), position)

    override fun close() {
        channel.close()
        stream.close()
        descriptor.close()
    }
}
