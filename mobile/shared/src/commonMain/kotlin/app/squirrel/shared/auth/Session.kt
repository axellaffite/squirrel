package app.squirrel.shared.auth

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

sealed interface SessionState {
    /** Not read from the [TokenStore] yet. */
    data object Unknown : SessionState
    data object SignedOut : SessionState
    data object SignedIn : SessionState
}

/** A fresh token pair, as returned by device login and refresh. */
data class Tokens(val accessToken: String, val refreshToken: String)

/** What a refresh call answered. */
sealed interface RefreshOutcome {
    data class Refreshed(val tokens: Tokens) : RefreshOutcome
    /** The server refused the refresh token (revoked, expired, reused): signed out. */
    data object Rejected : RefreshOutcome
}

/**
 * The device's credentials: the refresh token in [store], the access token in memory only.
 * Refreshes are single-flight: callers that saw the same stale access token share one refresh.
 */
class Session(private val store: TokenStore) {
    private val mutex = Mutex()
    private var access: String? = null
    private val _state = MutableStateFlow<SessionState>(SessionState.Unknown)
    val state: StateFlow<SessionState> = _state.asStateFlow()

    /** Reads the store once at start-up. */
    suspend fun restore(): SessionState = mutex.withLock {
        val next = if (store.load() != null) SessionState.SignedIn else SessionState.SignedOut
        _state.value = next
        next
    }

    suspend fun signedIn(tokens: Tokens) = mutex.withLock {
        store.save(tokens.refreshToken)
        access = tokens.accessToken
        _state.value = SessionState.SignedIn
    }

    /** The access token in memory, if any. */
    suspend fun accessToken(): String? = mutex.withLock { access }

    /**
     * Gets a new access token after [stale] was refused. If another caller already replaced
     * [stale], its token is returned without a second refresh. Null when signed out.
     * Network failures from [call] propagate and leave the session as it was.
     */
    suspend fun refresh(stale: String?, call: suspend (refreshToken: String) -> RefreshOutcome): String? =
        mutex.withLock {
            val current = access
            if (current != null && current != stale) return@withLock current
            val refreshToken = store.load() ?: return@withLock signOutLocked()
            when (val outcome = call(refreshToken)) {
                is RefreshOutcome.Refreshed -> {
                    store.save(outcome.tokens.refreshToken)
                    access = outcome.tokens.accessToken
                    _state.value = SessionState.SignedIn
                    outcome.tokens.accessToken
                }
                RefreshOutcome.Rejected -> signOutLocked()
            }
        }

    /** Forgets every credential. */
    suspend fun signOut() {
        mutex.withLock { signOutLocked() }
    }

    private suspend fun signOutLocked(): String? {
        store.clear()
        access = null
        _state.value = SessionState.SignedOut
        return null
    }
}
