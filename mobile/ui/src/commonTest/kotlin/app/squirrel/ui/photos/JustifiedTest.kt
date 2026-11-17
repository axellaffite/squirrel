package app.squirrel.ui.photos

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** The web's vectors (web/src/lib/photos/justified.test.ts). */
class JustifiedTest {
    private val landscape = 3.0 / 2
    private val portrait = 3.0 / 4
    private fun layout(aspects: List<Double>) = justify(aspects, width = 1000, targetHeight = 200, gap = 4)

    @Test
    fun fillsFullRowsToTheWidthAndKeepsTheLastRowAtTheTarget() {
        val (rows, height) = layout(listOf(landscape, landscape, landscape, landscape, portrait))
        assertEquals(2, rows.size)
        assertEquals(0, rows[0].top)
        assertEquals(165, rows[0].height)
        assertEquals(listOf(247, 247, 247, 247), rows[0].tiles.map { it.width })
        val last = rows[0].tiles[3]
        assertEquals(1000, last.left + last.width)
        assertEquals(169, rows[1].top)
        assertEquals(200, rows[1].height)
        assertEquals(PlacedTile(4, 0, 150), rows[1].tiles[0])
        assertEquals(369, height)
    }

    @Test
    fun neverMakesAFullRowTallerThanTheTarget() {
        val (rows) = layout(List(30) { 0.6 + (it % 5) * 0.4 })
        rows.dropLast(1).forEach { row ->
            assertTrue(row.height <= 200)
            val end = row.tiles.last()
            assertEquals(1000, end.left + end.width)
        }
    }

    @Test
    fun givesNothingForNoTiles() {
        assertEquals(Justified(emptyList(), 0), layout(emptyList()))
    }

    @Test
    fun clampsPanoramasAndTreatsUnknownAspectsAsSquare() {
        assertEquals(600, layout(listOf(20.0)).rows[0].tiles[0].width)
        assertEquals(200, layout(listOf(Double.NaN)).rows[0].tiles[0].width)
    }

    @Test
    fun placeholderRowsAreSizedFromTheCount() {
        // 375 dp wide, 112 dp rows, 4 dp gaps: three square tiles per row.
        assertEquals(0, placeholderRows(0, 375, 112, 4))
        assertEquals(1, placeholderRows(3, 375, 112, 4))
        assertEquals(34, placeholderRows(100, 375, 112, 4))
    }
}
