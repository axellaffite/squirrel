package app.squirrel.shared.testing

import app.squirrel.shared.sync.AssetReader

/** Deterministic bytes: byte i is (i * 31 + seed) mod 256. */
class FakeReader(override val size: Long, private val seed: Int = 7) : AssetReader {
    var reads = 0
        private set
    var closed = false
        private set

    fun byteAt(position: Long): Byte = ((position * 31 + seed) % 256).toByte()

    fun bytes(from: Long, length: Int): ByteArray = ByteArray(length) { byteAt(from + it) }

    override suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int {
        if (position >= size) return -1
        reads++
        val n = minOf(length.toLong(), size - position).toInt()
        for (i in 0 until n) into[offset + i] = byteAt(position + i)
        return n
    }

    override fun close() {
        closed = true
    }
}
