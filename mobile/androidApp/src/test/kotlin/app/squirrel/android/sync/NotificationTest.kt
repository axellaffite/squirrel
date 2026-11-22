package app.squirrel.android.sync

import android.app.Application
import android.app.Notification
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.squirrel.shared.sync.SyncState
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import kotlin.test.assertEquals

@RunWith(AndroidJUnit4::class)
@Config(sdk = [35], application = Application::class)
class NotificationTest {
    private val factory = ForegroundInfoFactory(ApplicationProvider.getApplicationContext())

    private fun text(state: SyncState) = factory.notification(state).extras.getString(Notification.EXTRA_TEXT)

    @Test
    fun theTextComesFromResources() {
        assertEquals("Backing up 13 of 340", text(SyncState.Uploading("a.jpg", done = 12, of = 340)))
        assertEquals("Preparing 1 item", text(SyncState.Hashing(0, 1)))
        assertEquals("Preparing 40 items", text(SyncState.Hashing(0, 40)))
        assertEquals("Checking what is new", text(SyncState.Scanning))
        assertEquals("Waiting for network…", text(SyncState.WaitingForNetwork))
    }
}
