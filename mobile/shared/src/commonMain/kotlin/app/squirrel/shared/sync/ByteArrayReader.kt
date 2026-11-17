package app.squirrel.shared.sync

/** An [AssetReader] over bytes in memory (tests, small generated files). */
class ByteArrayReader(private val bytes: ByteArray, override val modifiedAt: Long? = null) : AssetReader {
    override val size: Long get() = bytes.size.toLong()

    override suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int {
        if (position >= bytes.size) return -1
        val count = minOf(length, bytes.size - position.toInt())
        bytes.copyInto(into, offset, position.toInt(), position.toInt() + count)
        return count
    }

    override fun close() = Unit
}
