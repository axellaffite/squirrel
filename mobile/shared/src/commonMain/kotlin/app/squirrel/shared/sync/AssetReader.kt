package app.squirrel.shared.sync

/** Random access to one asset's bytes (chunks must be re-readable to resume). */
interface AssetReader : AutoCloseable {
    val size: Long

    /** The file's modification time (epoch ms) as the platform reports it now, if known. */
    val modifiedAt: Long? get() = null

    /** The platform handed out the bytes without the photo's location (no original access). */
    val locationStripped: Boolean get() = false

    /** Reads up to [length] bytes at [position] into [into]; -1 at the end. */
    @Throws(Exception::class)
    suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int
}
