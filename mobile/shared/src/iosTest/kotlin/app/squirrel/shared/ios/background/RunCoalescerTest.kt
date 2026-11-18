package app.squirrel.shared.ios.background

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

class RunCoalescerTest {
    @Test
    fun requestsDuringARunCollapseIntoOneMoreRun() = runTest {
        val runs = mutableListOf<Boolean>()
        var release = CompletableDeferred<Unit>()
        val coalescer = RunCoalescer(this) { manual ->
            runs += manual
            release.await()
        }

        coalescer.request(manual = false)
        testScheduler.runCurrent()
        repeat(5) { coalescer.request(manual = false) }
        coalescer.request(manual = true)
        assertEquals(listOf(false), runs) // still one run

        val first = release
        release = CompletableDeferred()
        first.complete(Unit)
        testScheduler.runCurrent()
        assertEquals(listOf(false, true), runs) // one more, manual since one request was

        release.complete(Unit)
        testScheduler.runCurrent()
        assertEquals(listOf(false, true), runs)

        coalescer.request(manual = false) // idle again: a new run starts
        testScheduler.runCurrent()
        assertEquals(listOf(false, true, false), runs)
    }

    @Test
    fun aFailedRunDoesNotBlockTheNext() = runTest {
        var calls = 0
        val coalescer = RunCoalescer(backgroundScope) { calls++; error("offline") }
        coalescer.request(manual = false)
        testScheduler.runCurrent()
        coalescer.request(manual = false)
        testScheduler.runCurrent()
        assertEquals(2, calls)
    }
}
