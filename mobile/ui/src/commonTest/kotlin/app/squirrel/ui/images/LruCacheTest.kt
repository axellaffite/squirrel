package app.squirrel.ui.images

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class LruCacheTest {
    @Test
    fun aReadKeepsAnEntryAndTheLeastRecentlyUsedGoes() = runTest {
        val cache = LruCache<String, Int>(2)
        cache.put("a", 1)
        cache.put("b", 2)
        assertEquals(1, cache.get("a")) // a is now the most recent
        cache.put("c", 3)
        assertNull(cache.get("b"))
        assertEquals(1, cache.get("a"))
        assertEquals(3, cache.get("c"))
    }

    @Test
    fun manyThreadsAtOnceKeepItBounded() = runTest {
        val cache = LruCache<Int, Int>(16)
        withContext(Dispatchers.Default) {
            (0 until 2_000).map { i -> async { cache.put(i % 64, i); cache.get((i * 7) % 64) } }.awaitAll()
        }
        assertEquals(16, (0 until 64).count { cache.get(it) != null })
    }
}
