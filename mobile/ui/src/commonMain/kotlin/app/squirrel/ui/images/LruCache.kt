package app.squirrel.ui.images

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** A small least-recently-used cache, safe from any thread: a read moves its entry to the back. */
class LruCache<K : Any, V : Any>(private val capacity: Int) {
    private val mutex = Mutex()
    private val entries = LinkedHashMap<K, V>()

    suspend fun get(key: K): V? = mutex.withLock {
        val value = entries.remove(key) ?: return@withLock null
        entries[key] = value
        value
    }

    suspend fun put(key: K, value: V) = mutex.withLock {
        entries.remove(key)
        entries[key] = value
        while (entries.size > capacity) entries.remove(entries.keys.first())
    }
}
