package app.squirrel.shared.auth

import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class SessionTest {
    private val store = InMemoryTokenStore()
    private val session = Session(store)

    @Test
    fun theRefreshTokenGoesToTheStoreAndTheAccessTokenStaysInMemory() = runTest {
        session.signedIn(Tokens("sqa_1", "sqr_1"))

        assertEquals("sqr_1", store.load())
        assertEquals("sqa_1", session.accessToken())
        // A new process: the store remembers the refresh token, nothing remembers the access token.
        val restarted = Session(store)
        assertEquals(SessionState.SignedIn, restarted.restore())
        assertNull(restarted.accessToken())
    }

    @Test
    fun signOutClearsEverything() = runTest {
        session.signedIn(Tokens("sqa_1", "sqr_1"))

        session.signOut()

        assertNull(store.load())
        assertNull(session.accessToken())
        assertEquals(SessionState.SignedOut, session.state.value)
    }

    @Test
    fun aRefreshThatAnotherCallerAlreadyDidIsNotRepeated() = runTest {
        session.signedIn(Tokens("sqa_1", "sqr_1"))
        var calls = 0
        val call: suspend (String) -> RefreshOutcome = {
            calls++
            RefreshOutcome.Refreshed(Tokens("sqa_2", "sqr_2"))
        }

        assertEquals("sqa_2", session.refresh("sqa_1", call))
        assertEquals("sqa_2", session.refresh("sqa_1", call))

        assertEquals(1, calls)
        assertEquals("sqr_2", store.load())
    }

    @Test
    fun withoutARefreshTokenRefreshSignsOut() = runTest {
        assertNull(session.refresh(null) { error("must not be called") })
        assertEquals(SessionState.SignedOut, session.state.value)
    }

    @Test
    fun anUnavailableStoreKeepsTheDeviceSignedIn() = runTest {
        val flaky = object : TokenStore {
            override suspend fun load(): String? = throw TokenStoreUnavailable(IllegalStateException("keystore busy"))
            override suspend fun save(refreshToken: String) = Unit
            override suspend fun clear() = error("must not clear")
        }
        assertEquals(SessionState.SignedIn, Session(flaky).restore())
    }

    @Test
    fun aFailedSaveAfterRotationNeverFallsBackToTheOldToken() = runTest {
        val disk = InMemoryTokenStore("sqr_1")
        var failSaves = 1
        val flaky = object : TokenStore by disk {
            override suspend fun save(refreshToken: String) {
                if (failSaves-- > 0) error("disk full")
                disk.save(refreshToken)
            }
        }
        val session = Session(flaky)
        session.restore()
        val presented = mutableListOf<String>()
        var next = 1
        val call: suspend (String) -> RefreshOutcome = { token ->
            presented += token
            next++
            RefreshOutcome.Refreshed(Tokens("sqa_$next", "sqr_$next"))
        }

        assertEquals("sqa_2", session.refresh(null, call)) // rotated, but the save failed
        assertEquals("sqr_1", disk.load())
        assertEquals("sqa_3", session.refresh("sqa_2", call)) // the next refresh presents sqr_2

        assertEquals(listOf("sqr_1", "sqr_2"), presented)
        assertEquals("sqr_3", disk.load())
    }

    @Test
    fun aPendingTokenIsSavedOnTheNextRequest() = runTest {
        val disk = InMemoryTokenStore("sqr_1")
        var failSaves = 1
        val flaky = object : TokenStore by disk {
            override suspend fun save(refreshToken: String) {
                if (failSaves-- > 0) error("disk full")
                disk.save(refreshToken)
            }
        }
        val session = Session(flaky)
        session.refresh(null) { RefreshOutcome.Refreshed(Tokens("sqa_2", "sqr_2")) }

        assertEquals("sqa_2", session.accessToken())

        assertEquals("sqr_2", disk.load())
    }

    @Test
    fun aRefreshThatNeverAnswersTimesOutAndKeepsTheSession() = runTest {
        session.signedIn(Tokens("sqa_1", "sqr_1"))
        kotlin.test.assertFailsWith<RefreshTimedOut> {
            session.refresh("sqa_1") { kotlinx.coroutines.awaitCancellation() }
        }
        assertEquals(SessionState.SignedIn, session.state.value)
        assertEquals(false, session.refused()) // transient: the device stays signed in
    }

    /** A Keychain that is locked: reads work from memory, deleting fails. */
    private class LockedStore(var token: String?) : TokenStore {
        var locked = true
        override suspend fun load(): String? = token
        override suspend fun save(refreshToken: String) { token = refreshToken }
        override suspend fun clear() {
            if (locked) throw TokenStoreUnavailable(IllegalStateException("locked"))
            token = null
        }
    }

    private class FlagPendingClear : PendingClear {
        var pending = false
        override suspend fun isPending() = pending
        override suspend fun set(pending: Boolean) { this.pending = pending }
    }

    @Test
    fun aSignOutTheStoreRefusesStillSignsOutAndIsRetriedAtTheNextLaunch() = runTest {
        val locked = LockedStore("sqr_1")
        val flag = FlagPendingClear()
        val first = Session(locked, flag)
        first.signedIn(Tokens("sqa_1", "sqr_1"))

        first.signOut()

        assertEquals(SessionState.SignedOut, first.state.value)
        assertNull(first.accessToken())
        assertEquals(true, flag.pending)
        // Next launch, still locked: the leftover token never signs the app back in.
        assertEquals(SessionState.SignedOut, Session(locked, flag).restore())
        assertEquals(true, flag.pending)
        // Unlocked: the leftover is deleted and the flag dropped.
        locked.locked = false
        assertEquals(SessionState.SignedOut, Session(locked, flag).restore())
        assertNull(locked.token)
        assertEquals(false, flag.pending)
    }
}
