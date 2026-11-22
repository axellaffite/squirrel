package app.squirrel.ui

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

class ResumeThrottleTest {
    private var now = Duration.ZERO
    private val throttle = ResumeThrottle(now = { now })

    @Test
    fun aResumeRightAfterTheFirstLoadDoesNothing() {
        now = 5.seconds
        assertFalse(throttle.due())
    }

    @Test
    fun aResumeAfterThirtySecondsReloadsOnce() {
        now = 30.seconds
        assertTrue(throttle.due())
        assertFalse(throttle.due()) // counted: the next one waits again
        now = 59.seconds
        assertFalse(throttle.due())
        now = 60.seconds
        assertTrue(throttle.due())
    }

    @Test
    fun anyLoadPushesTheNextResumeBack() {
        now = 25.seconds
        throttle.loaded() // a pull, a folder opened
        now = 40.seconds
        assertFalse(throttle.due())
        now = 55.seconds
        assertTrue(throttle.due())
    }
}
