package app.squirrel.android.media

import org.junit.Test
import java.time.ZoneId
import kotlin.test.assertEquals
import kotlin.test.assertNull

class TakenAtTest {
    @Test
    fun takenAtCarriesThePhonesOffsetAtThatInstant() {
        val paris = ZoneId.of("Europe/Paris")
        assertEquals("2024-07-14T18:30:05+02:00", takenAt(1_720_974_605_000, paris)) // summer
        assertEquals("2024-01-14T18:30:05+01:00", takenAt(1_705_253_405_000, paris)) // winter
        assertEquals("2024-07-14T16:30:05Z", takenAt(1_720_974_605_123, ZoneId.of("UTC")))
        assertNull(takenAt(0, paris))
    }
}
