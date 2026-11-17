package app.squirrel.shared.ios.photos

import platform.Photos.PHAuthorizationStatusAuthorized
import platform.Photos.PHAuthorizationStatusDenied
import platform.Photos.PHAuthorizationStatusLimited
import platform.Photos.PHAuthorizationStatusNotDetermined
import platform.Photos.PHAuthorizationStatusRestricted
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class PhotoAccessTest {
    @Test
    fun onlyFullOrLimitedAccessLetsTheLibraryBeObserved() {
        assertTrue(isGranted(PHAuthorizationStatusAuthorized))
        assertTrue(isGranted(PHAuthorizationStatusLimited))
        assertFalse(isGranted(PHAuthorizationStatusNotDetermined))
        assertFalse(isGranted(PHAuthorizationStatusDenied))
        assertFalse(isGranted(PHAuthorizationStatusRestricted))
    }
}
