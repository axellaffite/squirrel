package app.squirrel.shared.ios.transfer

import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.testing.testDriver
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.io.IOException
import platform.Foundation.NSMutableURLRequest
import platform.Foundation.NSTemporaryDirectory
import platform.Foundation.NSURL
import platform.Foundation.NSURLSessionConfiguration
import platform.Foundation.NSUUID
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse

class TransferSessionTest {
    private val transfers = TransferSession(
        NSURLSessionConfiguration.ephemeralSessionConfiguration,
        TransportStore(createDatabase(testDriver())) { 0L },
        "${NSTemporaryDirectory()}chunks-${NSUUID().UUIDString}",
    )
    private val request = NSMutableURLRequest(NSURL(string = "http://127.0.0.1:9/api/uploads/u/chunks/0"))

    @Test
    fun aChunkThatFailedToBeWrittenIsNotLeftWaiting() = runBlocking {
        val failure = assertFailsWith<IOException> {
            transfers.send("u:0", request) { throw IOException("disk full") }
        }
        assertEquals("disk full", failure.message)
        assertFalse(transfers.isWaiting("u:0"))
        // A retry writes again instead of hanging on the first attempt's waiter.
        var writes = 0
        withTimeout(5_000) {
            assertFailsWith<IOException> { transfers.send("u:0", request) { writes++; throw IOException("still full") } }
        }
        assertEquals(1, writes)
    }

    @Test
    fun aSenderCancelledBeforeTheTaskStartedLeavesNoWaiter() = runBlocking {
        val writing = CompletableDeferred<Unit>()
        val sender = launch {
            transfers.send("u:1", request) {
                writing.complete(Unit)
                awaitCancellation()
            }
        }
        writing.await()
        sender.cancel()
        sender.join()
        assertFalse(transfers.isWaiting("u:1"))
    }
}
