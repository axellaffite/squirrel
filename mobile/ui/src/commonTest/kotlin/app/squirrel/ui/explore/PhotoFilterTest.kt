package app.squirrel.ui.explore

import app.squirrel.api.models.AssetKindDto
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.CameraGroupDto
import app.squirrel.api.models.FaceBoxDto
import app.squirrel.api.models.PhotoKindFilter
import app.squirrel.api.models.PlaceGroupDto
import app.squirrel.api.models.ThumbnailState
import app.squirrel.api.models.ThumbnailStatus
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class PhotoFilterTest {
    private val cover = AssetSummary(id = "p", kind = AssetKindDto.photo, name = "p.jpg", takenAt = "2024-07-14T18:30:05", thumbnail = ThumbnailState(ThumbnailStatus.ready, 1))

    private fun place(city: String?, region: String?) =
        PlaceGroupDto(countryCode = "FR", count = 3, cover = cover, city = city, country = "France", region = region)

    @Test
    fun partsThatNeedAnotherAreDropped() {
        val f = PhotoFilter(region = "Bretagne", city = "Brest", cameraModel = "R6", tag = "  ").normalized()
        assertEquals(PhotoFilter(), f)
        assertTrue(f.isEmpty)
        assertEquals(PhotoFilter(country = "FR", region = ""), PhotoFilter(country = " FR ", region = "").normalized())
    }

    @Test
    fun chipsNameEachPartAndRemoveIt() {
        val filter = PhotoFilter(person = "anna", tag = "birthday-cake", country = "FR", region = "", city = "Brest", cameraMake = "Canon", cameraModel = "R6", kind = PhotoKindFilter.photo)
        val list = chips(filter) { if (it == "anna") "Anna" else null }
        assertEquals(listOf("Anna", "Birthday cake", "Brest", "Canon R6", "Photos only"), list.map { it.label })
        assertEquals(null, list[2].without.country)
        assertEquals(null, list[2].without.city)
        assertEquals("Unnamed", chips(PhotoFilter(person = "x")).single().label)
    }

    @Test
    fun placesAndCamerasBecomeFilters() {
        assertEquals(PhotoFilter(country = "FR"), placeFilter(place(null, null)))
        assertEquals(PhotoFilter(country = "FR", region = "", city = "Brest"), placeFilter(place("Brest", null)))
        assertEquals("Brest", placeName(place("Brest", "Bretagne")))
        assertEquals("France", placeName(place(null, null)))
        val camera = CameraGroupDto(count = 2, make = "Canon", model = null)
        assertEquals(PhotoFilter(tag = "dog", cameraMake = "Canon"), PhotoFilter(tag = "dog").withCamera(camera))
        assertEquals("Canon", cameraLabel(camera))
        assertEquals(listOf("France" to 2), placesByCountry(listOf(place("Brest", null), place("Paris", null))).map { it.first to it.second.size })
    }

    @Test
    fun theKeyTellsFiltersApart() {
        assertEquals(PhotoFilter(tag = "dog").key, PhotoFilter(tag = " dog ").key)
        assertTrue(PhotoFilter(tag = "dog").key != PhotoFilter(person = "dog").key)
        assertEquals("1 photo", photoCount(1))
        assertEquals("Unnamed", personLabel(" "))
    }

    @Test
    fun theCropKeepsRoomAroundTheFaceInsideThePhoto() {
        val window = cropWindow(FaceBoxDto(height = 0.2f, width = 0.1f, x = 0.45f, y = 0.4f))
        assertEquals(0.3, window.height, 1e-6)
        assertEquals(0.35, window.top, 1e-6)
        assertEquals(0.5, window.centre, 1e-6)
        // A face at the corner: the window is pushed back inside.
        val corner = cropWindow(FaceBoxDto(height = 0.2f, width = 0.1f, x = 0f, y = 0f), aspect = 1.5)
        assertEquals(0.0, corner.top, 1e-6)
        assertEquals(corner.width / 2, corner.centre, 1e-6)
    }
}
