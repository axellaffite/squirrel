package app.squirrel.shared.ios.transfer

import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.testing.testDriver
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class TransportStoreTest {
    private var clock = 1_000_000L
    private val store = TransportStore(createDatabase(testDriver())) { clock }

    @Test
    fun aResultIsConsumedExactlyOnce() {
        store.record("up-1:0", ChunkResult(204, null))
        store.record("up-1:1", ChunkResult(0, "The network connection was lost."))
        assertEquals(ChunkResult(204, null), store.take("up-1:0"))
        assertNull(store.take("up-1:0"))
        assertEquals(ChunkResult(0, "The network connection was lost."), store.take("up-1:1"))
        assertNull(store.take("up-2:0"))
    }

    @Test
    fun aNewerResultReplacesAnOlderOne() {
        store.record("up-1:0", ChunkResult(500, null))
        store.record("up-1:0", ChunkResult(204, null))
        assertEquals(204, store.take("up-1:0")?.status)
    }

    @Test
    fun oldResultsArePruned() {
        store.record("up-1:0", ChunkResult(204, null))
        clock += RESULT_KEEP_MS + 1
        store.record("up-1:1", ChunkResult(204, null))
        store.prune()
        assertNull(store.take("up-1:0"))
        assertEquals(204, store.take("up-1:1")?.status)
    }
}
