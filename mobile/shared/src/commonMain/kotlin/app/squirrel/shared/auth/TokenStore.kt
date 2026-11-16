package app.squirrel.shared.auth

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Where the device's refresh token lives: the Android Keystore-encrypted `secret` row, the iOS
 * Keychain. Access tokens are never stored (they stay in [Session]'s memory).
 */
interface TokenStore {
    suspend fun load(): String?
    suspend fun save(refreshToken: String)
    suspend fun clear()
}

/** A [TokenStore] that forgets everything with the process (tests, previews). */
class InMemoryTokenStore(initial: String? = null) : TokenStore {
    private val mutex = Mutex()
    private var token: String? = initial

    override suspend fun load(): String? = mutex.withLock { token }
    override suspend fun save(refreshToken: String) = mutex.withLock { token = refreshToken }
    override suspend fun clear() = mutex.withLock { token = null }
}
