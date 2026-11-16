package app.squirrel.shared.hash

import app.squirrel.shared.sync.AssetReader
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

const val HASH_BUFFER = 1 shl 20

/** Hashes [reader] in [bufferSize] steps without holding the file in memory. */
suspend fun hashAsset(reader: AssetReader, bufferSize: Int = HASH_BUFFER, onProgress: (Long) -> Unit = {}): String {
    val sha = Sha256()
    val buffer = ByteArray(bufferSize)
    var position = 0L
    while (position < reader.size) {
        currentCoroutineContext().ensureActive()
        val n = reader.read(position, buffer, 0, minOf(bufferSize.toLong(), reader.size - position).toInt())
        if (n < 0) break
        sha.update(buffer, 0, n)
        position += n
        onProgress(position)
    }
    check(position == reader.size) { "The file is shorter than its recorded size" }
    return sha.digestHex()
}
