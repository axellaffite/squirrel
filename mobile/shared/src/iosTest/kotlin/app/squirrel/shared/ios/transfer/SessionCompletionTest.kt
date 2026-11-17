package app.squirrel.shared.ios.transfer

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SessionCompletionTest {
    private val completion = SessionCompletion()

    @Test
    fun theHandlerIsStoredBeforeTheSessionIsCreated() {
        var storedWhenCreated = false
        completion.onEvents(UPLOAD_SESSION_ID, handler = {}) { storedWhenCreated = completion.isPending }
        assertTrue(storedWhenCreated)
    }

    @Test
    fun theHandlerIsCalledOnce() {
        var calls = 0
        completion.onEvents(UPLOAD_SESSION_ID, handler = { calls++ }) {}
        completion.finish()
        completion.finish()
        assertEquals(1, calls)
        assertFalse(completion.isPending)
    }

    @Test
    fun anotherSessionsHandlerIsCalledAtOnceWithoutCreatingOurs() {
        var calls = 0
        var created = false
        completion.onEvents("com.example.other", handler = { calls++ }) { created = true }
        assertEquals(1, calls)
        assertFalse(created)
        assertFalse(completion.isPending)
    }

    @Test
    fun finishingWithNothingPendingDoesNothing() {
        completion.finish()
        var calls = 0
        completion.onEvents(UPLOAD_SESSION_ID, handler = { calls++ }) {}
        assertEquals(0, calls)
        completion.finish()
        assertEquals(1, calls)
    }
}
