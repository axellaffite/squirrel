package app.squirrel.ui.photos

import androidx.compose.ui.geometry.Offset
import app.squirrel.api.models.AssetKindDto
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.ThumbnailState
import app.squirrel.api.models.ThumbnailStatus
import kotlin.test.Test
import kotlin.test.assertEquals

class LabelsTest {
    private fun asset(mime: String) = AssetSummary(
        id = "a",
        kind = AssetKindDto.photo,
        name = "a",
        takenAt = "2026-10-12T19:59:03",
        thumbnail = ThumbnailState(ThumbnailStatus.ready, 1),
        mime = mime,
    )

    @Test
    fun aTileSaysWhatItIsAndWhen() {
        assertEquals("Photo, 12 October 2026, 19:59", photoLabel(asset("image/jpeg")))
        assertEquals("Video, 12 October 2026, 19:59", photoLabel(asset("video/mp4")))
    }

    @Test
    fun aZoomedPhotoNeverPansPastItsEdges() {
        // At 2×, a 400 × 300 stage lets the photo move 200 by 150 each way, no more.
        assertEquals(Offset(200f, -150f), clampPan(Offset(900f, -900f), 2f, 400f, 300f))
        assertEquals(Offset(10f, 20f), clampPan(Offset(10f, 20f), 2f, 400f, 300f))
        assertEquals(Offset.Zero, clampPan(Offset(50f, 50f), 1f, 400f, 300f))
    }
}
