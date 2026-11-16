package app.squirrel.shared.sync

/** Random access to one asset's bytes (chunks must be re-readable to resume). */
interface AssetReader : AutoCloseable {
    val size: Long

    /** Reads up to [length] bytes at [position] into [into]; -1 at the end. */
    suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int
}
