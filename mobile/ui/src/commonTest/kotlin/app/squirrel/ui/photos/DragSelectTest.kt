package app.squirrel.ui.photos

import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.AssetKindDto
import app.squirrel.api.models.ThumbnailState
import app.squirrel.api.models.ThumbnailStatus
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class DragSelectTest {
    private val order = listOf("a", "b", "c", "d", "e")

    @Test
    fun aLongPressAloneSelectsTheTileUnderTheFinger() {
        val drag = DragSelection.start(emptySet(), "c")
        assertEquals(setOf("c"), drag.over(order, "c"))
    }

    @Test
    fun slidingSelectsEveryPhotoBetweenTheAnchorAndTheFingerInEitherDirection() {
        val drag = DragSelection.start(setOf("a"), "b")
        assertEquals(setOf("a", "b", "c", "d"), drag.over(order, "d"))
        assertEquals(setOf("a", "b"), drag.over(order, "a"))
    }

    @Test
    fun slidingBackGivesTheLeftTilesTheirFormerState() {
        val drag = DragSelection.start(setOf("e"), "a")
        assertEquals(setOf("a", "b", "c", "d", "e"), drag.over(order, "e"))
        assertEquals(setOf("a", "b", "e"), drag.over(order, "b"))
    }

    @Test
    fun startingOnASelectedPhotoUnselectsTheRange() {
        val drag = DragSelection.start(setOf("a", "b", "c", "d"), "b")
        assertEquals(setOf("a", "d"), drag.over(order, "c"))
        assertEquals(setOf("a", "c", "d"), drag.over(order, "b"))
    }

    @Test
    fun anAnchorGoneFromTheListLeavesTheSelectionAlone() {
        val drag = DragSelection.start(setOf("a"), "gone")
        assertEquals(setOf("a"), drag.over(order, "c"))
    }

    private fun asset(id: String) =
        AssetSummary(id = id, kind = AssetKindDto.photo, name = "$id.jpg", takenAt = "2024-07-14T18:30:05", thumbnail = ThumbnailState(ThumbnailStatus.ready, 1))

    private val rows = listOf(RowSpan("h:2024-07", 0, 40), RowSpan("r:2024-07:0", 40, 140), RowSpan("r:2024-07:1", 144, 244))
    private val tiles = mapOf(
        "r:2024-07:0" to listOf(TileSlot(asset("a"), 0, 100), TileSlot(asset("b"), 104, 96)),
        "r:2024-07:1" to listOf(TileSlot(asset("c"), 0, 200)),
    )

    @Test
    fun theTileUnderAPointIsFoundInDpFromPixels() {
        assertEquals("a", tileAt(rows, x = 50f, y = 60f, density = 2f) { tiles[it] })
        assertEquals("b", tileAt(rows, x = 220f, y = 139f, density = 2f) { tiles[it] })
        assertEquals("c", tileAt(rows, x = 10f, y = 200f, density = 1f) { tiles[it] })
    }

    @Test
    fun headersGapsAndEmptySpaceHoldNoTile() {
        assertNull(tileAt(rows, x = 10f, y = 10f, density = 1f) { tiles[it] })
        assertNull(tileAt(rows, x = 10f, y = 142f, density = 1f) { tiles[it] })
        assertNull(tileAt(rows, x = 202f, y = 60f, density = 1f) { tiles[it] })
        assertNull(tileAt(rows, x = 10f, y = 900f, density = 1f) { tiles[it] })
    }

    @Test
    fun theListScrollsFasterTheDeeperTheFingerGoesIntoAnEdge() {
        assertEquals(0f, edgeScroll(y = 500f, top = 0f, bottom = 1000f, zone = 100f, maxStep = 20f))
        assertEquals(-10f, edgeScroll(y = 50f, top = 0f, bottom = 1000f, zone = 100f, maxStep = 20f))
        assertEquals(-20f, edgeScroll(y = -30f, top = 0f, bottom = 1000f, zone = 100f, maxStep = 20f))
        assertEquals(10f, edgeScroll(y = 950f, top = 0f, bottom = 1000f, zone = 100f, maxStep = 20f))
        assertEquals(20f, edgeScroll(y = 1200f, top = 0f, bottom = 1000f, zone = 100f, maxStep = 20f))
    }
}
