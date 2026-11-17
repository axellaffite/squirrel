package app.squirrel.android.media

import org.junit.Test
import kotlin.test.assertEquals

class ScanCursorTest {
    @Test
    fun cursorsRoundTripPerVolume() {
        val cursors = mapOf(
            "external_primary" to VolumeCursor("c6a8f7d5713ea722", 705),
            "1234-abcd" to VolumeCursor("0011aa", 12),
        )
        assertEquals(cursors, decodeCursors(encodeCursors(cursors)))
        assertEquals("1234-abcd=0011aa|12;external_primary=c6a8f7d5713ea722|705", encodeCursors(cursors))
    }

    @Test
    fun oldOrBrokenTokensMeanAFullScan() {
        assertEquals(emptyMap(), decodeCursors(null))
        assertEquals(emptyMap(), decodeCursors("c6a8f7d5713ea722|705")) // the single-volume format
        assertEquals(mapOf("v" to VolumeCursor("x", 3)), decodeCursors("v=x|3;broken"))
    }

    @Test
    fun theSnapshotIsTheNextCursorOnApi30() {
        assertEquals(900, nextPosition(since = 700, snapshot = 900, seen = listOf(750, 800)))
        assertEquals(900, nextPosition(since = null, snapshot = 900, seen = emptyList()))
    }

    @Test
    fun api29KeepsTheNewestTimeSeen() {
        assertEquals(1_700_000_500, nextPosition(since = 1_700_000_000, snapshot = null, seen = listOf(1_700_000_500)))
        assertEquals(1_700_000_000, nextPosition(since = 1_700_000_000, snapshot = null, seen = emptyList()))
    }
}
