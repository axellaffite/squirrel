package app.squirrel.shared.notifications

import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.db.forgetDevice
import app.squirrel.shared.settings.SettingsStore
import app.squirrel.shared.testing.testDriver
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.time.Instant

class SettingsAnnounceStoreTest {
    @Test
    fun theMarkIsKeptAndForgottenAtSignOut() = runTest {
        val db = createDatabase(testDriver())
        val settings = SettingsStore(db)
        val store = SettingsAnnounceStore(settings)
        assertNull(store.load())
        val mark = AnnounceMark(Instant.parse("2026-11-24T10:00:00Z"), setOf("a", "b"))
        store.save(mark)
        assertEquals(mark, store.load())
        db.forgetDevice()
        assertNull(store.load())
    }

    @Test
    fun aDamagedValueCountsAsNone() = runTest {
        val settings = SettingsStore(createDatabase(testDriver()))
        settings.setValue("notifications.announced", "{")
        assertNull(SettingsAnnounceStore(settings).load())
    }
}
