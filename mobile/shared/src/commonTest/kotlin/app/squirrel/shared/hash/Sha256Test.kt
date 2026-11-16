package app.squirrel.shared.hash

import app.squirrel.shared.sync.AssetReader
import app.squirrel.shared.testing.FakeReader
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

class Sha256Test {
    private fun sha(text: String) = Sha256().apply { update(text.encodeToByteArray()) }.digestHex()

    @Test
    fun nistVectors() {
        assertEquals("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", sha(""))
        assertEquals("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", sha("abc"))
        assertEquals(
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            sha("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        )
        assertEquals(
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
            sha(
                "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmno" +
                    "ijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            ),
        )
    }

    @Test
    fun aMillionAsInOddPieces() {
        val sha = Sha256()
        val chunk = ByteArray(4099) { 'a'.code.toByte() }
        var left = 1_000_000
        var step = 1
        while (left > 0) {
            val n = minOf(step, left, chunk.size)
            sha.update(chunk, offset = 1, length = n) // pieces of 1..4097 bytes, at an offset
            left -= n
            step = step * 3 % 4097 + 1
        }
        assertEquals("cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0", sha.digestHex())
    }

    @Test
    fun hashingAReaderEqualsHashingTheBytesAtOnce() = runTest {
        val reader = FakeReader(size = 20L * 1024 * 1024 + 123)
        val progress = mutableListOf<Long>()

        val streamed = hashAsset(reader, onProgress = { progress += it })

        val whole = Sha256().apply { update(reader.bytes(0, reader.size.toInt())) }.digestHex()
        assertEquals(whole, streamed)
        assertEquals(progress.sorted(), progress)
        assertEquals(reader.size, progress.last())
    }

    @Test
    fun cancellationStopsReading() = runTest {
        val reader = object : AssetReader {
            var reads = 0
            override val size = 100L * 1024 * 1024
            override suspend fun read(position: Long, into: ByteArray, offset: Int, length: Int): Int {
                reads++
                yield()
                return length
            }
            override fun close() = Unit
        }
        val job = async { hashAsset(reader, bufferSize = 1024) }
        while (reader.reads < 3) yield()
        job.cancel()
        assertFailsWith<CancellationException> { job.await() }
        assertTrue(reader.reads < 100 * 1024)
    }
}
