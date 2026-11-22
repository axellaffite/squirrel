package app.squirrel.theme.components

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class PullTrackerTest {
    @Test
    fun theIndicatorFollowsTheFingerAtHalfSpeed() {
        val pull = PullTracker(threshold = 100f)
        assertEquals(60f, pull.pull(60f)) // the whole overscroll is taken
        assertEquals(30f, pull.distance)
        assertEquals(0.3f, pull.fraction)
        assertFalse(pull.armed)
    }

    @Test
    fun lettingGoPastTheThresholdRefreshes() {
        val pull = PullTracker(threshold = 100f)
        pull.pull(200f)
        assertTrue(pull.armed)
        assertTrue(pull.release())
        assertFalse(pull.pulling)
    }

    @Test
    fun lettingGoShortOfItDoesNot() {
        val pull = PullTracker(threshold = 100f)
        pull.pull(199f)
        assertFalse(pull.release())
    }

    @Test
    fun aPullStopsAtOneAndAHalfThresholds() {
        val pull = PullTracker(threshold = 100f)
        pull.pull(1_000f)
        assertEquals(150f, pull.distance)
        assertEquals(1f, pull.fraction)
    }

    @Test
    fun scrollingBackUpPutsTheIndicatorAwayBeforeTheListMoves() {
        val pull = PullTracker(threshold = 100f)
        pull.pull(80f) // 40 shown
        assertEquals(-50f, pull.push(-50f)) // 25 of it taken back
        assertEquals(15f, pull.distance)
        assertEquals(-30f, pull.push(-100f)) // only what was left; the list gets the rest
        assertEquals(0f, pull.distance)
        assertFalse(pull.pulling)
        assertEquals(0f, pull.push(-10f))
    }

    @Test
    fun scrollingDownIsNotAPull() {
        val pull = PullTracker(threshold = 100f)
        assertEquals(0f, pull.pull(-20f))
        assertFalse(pull.pulling)
        assertFalse(pull.release())
    }

    @Test
    fun settlingNeverFightsTheFinger() {
        val pull = PullTracker(threshold = 100f)
        pull.pull(40f)
        pull.settle(0f)
        assertEquals(20f, pull.distance)
        pull.release()
        pull.settle(100f)
        assertEquals(100f, pull.distance)
    }
}
