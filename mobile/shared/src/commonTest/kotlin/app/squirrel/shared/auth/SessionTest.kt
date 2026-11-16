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
}
