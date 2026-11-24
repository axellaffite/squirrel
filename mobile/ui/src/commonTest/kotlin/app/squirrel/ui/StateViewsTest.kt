package app.squirrel.ui

import app.cash.turbine.test
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

class StateViewsTest {
    @Test
    fun aMappedStateFollowsItsSourceWithoutACollector() {
        val source = MutableStateFlow(1)
        val doubled = source.mapState { it * 2 }

        source.value = 5

        assertEquals(10, doubled.value)
    }

    @Test
    fun aCombinedStateEmitsEachDistinctChange() = runTest {
        val a = MutableStateFlow(1)
        val b = MutableStateFlow(10)
        val sum = combineState(a, b) { x, y -> x + y }

        sum.test {
            assertEquals(11, awaitItem())
            a.value = 2
            assertEquals(12, awaitItem())
            b.value = 10
            a.value = 3
            assertEquals(13, awaitItem())
        }
        assertEquals(13, sum.value)
    }
}
