package app.squirrel.shared.sync

/** Random access to one asset's bytes (chunks must be re-readable to resume). */
interface AssetReader : AutoCloseable {
    val size: Long

    /** The file's modification time (epoch ms) as the platform reports it now, if known. */
    val modifiedAt: Long? get() = null

    /** Reads up to [length] bytes at [position] into [into]; -1 at the end. */
    suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int
}
