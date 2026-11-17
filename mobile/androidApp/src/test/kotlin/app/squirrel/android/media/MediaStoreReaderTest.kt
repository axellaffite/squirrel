package app.squirrel.android.media

import android.os.ParcelFileDescriptor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.assertArrayEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The uploader reads chunks of one file in parallel: reads must not share a file position. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class MediaStoreReaderTest {
    @get:Rule
    val folder = TemporaryFolder()

    @Test
    fun chunksReadInParallelGetTheirOwnBytes() = runBlocking {
        val bytes = ByteArray(1 shl 20) { (it * 31 + it / 4096).toByte() }
        val file = folder.newFile("photo.bin").apply { writeBytes(bytes) }
        val reader = MediaStoreReader(ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY), file.length(), modifiedAt = 1)
        val chunk = 64 * 1024
        val results = withContext(Dispatchers.Default) {
            (0 until bytes.size / chunk).map { index ->
                async {
                    val into = ByteArray(chunk)
                    var filled = 0
                    while (filled < chunk) filled += reader.read(index.toLong() * chunk + filled, into, filled, chunk - filled)
                    index to into
                }
            }.awaitAll()
        }
        results.forEach { (index, got) -> assertArrayEquals("chunk $index", bytes.copyOfRange(index * chunk, (index + 1) * chunk), got) }
        reader.close()
    }
}
