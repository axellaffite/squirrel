package app.squirrel.shared.ios.photos

import platform.Foundation.NSString
import platform.Foundation.NSUTF8StringEncoding
import platform.Foundation.create
import platform.Foundation.dataUsingEncoding
import platform.Photos.PHAssetResourceTypeAdjustmentData
import platform.Photos.PHAssetResourceTypeFullSizePhoto
import platform.Photos.PHAssetResourceTypePairedVideo
import platform.Photos.PHAssetResourceTypePhoto
import platform.Photos.PHAssetResourceTypeVideo
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class AssetMappingTest {
    @Test
    fun anEditedPhotoBacksUpWhatTheUserSees() {
        val types = listOf(PHAssetResourceTypePhoto, PHAssetResourceTypeAdjustmentData, PHAssetResourceTypeFullSizePhoto)
        assertEquals(2, AssetMapping.primaryResource(types, video = false))
        assertEquals(0, AssetMapping.primaryResource(listOf(PHAssetResourceTypePhoto), video = false))
    }

    @Test
    fun aLivePhotoBacksUpItsStillOnly() {
        val types = listOf(PHAssetResourceTypePhoto, PHAssetResourceTypePairedVideo)
        assertEquals(0, AssetMapping.primaryResource(types, video = false))
        assertEquals(0, AssetMapping.primaryResource(listOf(PHAssetResourceTypeVideo), video = true))
        assertNull(AssetMapping.primaryResource(listOf(PHAssetResourceTypePairedVideo), video = true))
    }

    @Test
    fun mimeTypesComeFromUniformTypesOrTheName() {
        assertEquals("image/heic", AssetMapping.mime("public.heic", "IMG_0001.HEIC"))
        assertEquals("image/jpeg", AssetMapping.mime(null, "IMG_0002.JPG"))
        assertEquals("video/quicktime", AssetMapping.mime("com.apple.quicktime-movie", "IMG_0003.MOV"))
        assertNull(AssetMapping.mime(null, "README"))
    }

    @Test
    fun takenAtCarriesTheLocalOffset() {
        // 2024-07-14T16:30:05Z
        assertEquals("2024-07-14T18:30:05+02:00", AssetMapping.takenAt(1720974605.0, 7200))
        assertEquals("2024-07-14T16:30:05+00:00", AssetMapping.takenAt(1720974605.0, 0))
        assertEquals("2024-07-14T11:30:05-05:00", AssetMapping.takenAt(1720974605.0, -18000))
        assertEquals("2024-07-14T16:30:00+00:00", AssetMapping.takenAt(1720974600.0, 0))
    }

    @OptIn(kotlinx.cinterop.BetaInteropApi::class)
    @Test
    fun aTokenRoundTripsThroughText() {
        @Suppress("CAST_NEVER_SUCCEEDS")
        val bytes = ("token bytes" as NSString).dataUsingEncoding(NSUTF8StringEncoding)!!
        val back = AssetMapping.decode(AssetMapping.encode(bytes))!!
        assertEquals("token bytes", NSString.create(back, NSUTF8StringEncoding).toString())
        assertNull(AssetMapping.decode("%%% not base64"))
    }
}
