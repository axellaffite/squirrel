package app.squirrel.shared.auth

import kotlinx.coroutines.CancellationException
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
data class Tokens(val accessToken: String, val refreshToken: String) {
    override fun toString(): String = "Tokens(<redacted>)"
}

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
    /** A rotated refresh token the store failed to save; it replaces whatever the store holds. */
    private var pendingRefresh: String? = null
    /** The last refresh failed for a transient reason (network, proxy), not a refusal. */
    private var refreshFailedTransiently = false
    private val _state = MutableStateFlow<SessionState>(SessionState.Unknown)
    val state: StateFlow<SessionState> = _state.asStateFlow()

    /** Reads the store once at start-up. */
    suspend fun restore(): SessionState = mutex.withLock {
        val stored = try {
            store.load() != null
        } catch (_: TokenStoreUnavailable) {
            true // A token is there but unreadable now; the next refresh tries again.
        }
        val next = if (stored) SessionState.SignedIn else SessionState.SignedOut
        _state.value = next
        next
    }

    suspend fun signedIn(tokens: Tokens) = mutex.withLock {
        access = tokens.accessToken
        persist(tokens.refreshToken)
        _state.value = SessionState.SignedIn
    }

    /** The access token in memory, if any; also retries saving a pending refresh token. */
    suspend fun accessToken(): String? = mutex.withLock {
        pendingRefresh?.let { persist(it) }
        access
    }

    /**
     * Gets a new access token after [stale] was refused. If another caller already replaced
     * [stale], its token is returned without a second refresh. Null when signed out.
     * Network failures from [call] propagate and leave the session as it was.
     */
    suspend fun refresh(stale: String?, call: suspend (refreshToken: String) -> RefreshOutcome): String? =
        mutex.withLock {
            val current = access
            if (current != null && current != stale) return@withLock current
            // Never the store's copy while a newer token waits to be saved: the old one was rotated out.
            val refreshToken = pendingRefresh ?: store.load() ?: return@withLock signOutLocked()
            val outcome = try {
                call(refreshToken)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                refreshFailedTransiently = true
                throw e
            }
            refreshFailedTransiently = false
            when (outcome) {
                is RefreshOutcome.Refreshed -> {
                    access = outcome.tokens.accessToken
                    persist(outcome.tokens.refreshToken)
                    _state.value = SessionState.SignedIn
                    outcome.tokens.accessToken
                }
                RefreshOutcome.Rejected -> signOutLocked()
            }
        }

    /**
     * The server refused this device after the refresh and retry. Signs out and returns true,
     * unless that refresh failed transiently (the refusal may only be an expired access token).
     */
    suspend fun refused(): Boolean = mutex.withLock {
        when {
            _state.value == SessionState.SignedOut -> true
            refreshFailedTransiently -> false
            else -> {
                signOutLocked()
                true
            }
        }
    }

    /** Forgets every credential. */
    suspend fun signOut() {
        mutex.withLock { signOutLocked() }
    }

    /** Saves [refreshToken]; on failure keeps it pending in memory and retries on the next request. */
    private suspend fun persist(refreshToken: String) {
        pendingRefresh = refreshToken
        try {
            store.save(refreshToken)
            pendingRefresh = null
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            // Kept in pendingRefresh: used for the next refresh and saved again before then.
        }
    }

    private suspend fun signOutLocked(): String? {
        pendingRefresh = null
        access = null
        store.clear()
        _state.value = SessionState.SignedOut
        return null
    }
}
