package app.squirrel.android.sync

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.work.Configuration
import androidx.work.WorkInfo
import androidx.work.WorkManager
import org.robolectric.shadows.ShadowLooper
import java.util.concurrent.Executors
import androidx.work.testing.WorkManagerTestInitHelper
import app.squirrel.shared.settings.BackupSettings
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals

/** The content-URI trigger stays armed: after the run it triggered, and after a cold start. */
@RunWith(AndroidJUnit4::class)
@Config(sdk = [35], application = FakeBackupApp::class)
class ContentTriggerTest {
    private val app = ApplicationProvider.getApplicationContext<FakeBackupApp>()
    private lateinit var work: WorkManager
    private lateinit var scheduler: BackupScheduler
    private val settings = BackupSettings()

    @Before
    fun setUp() {
        // Real threads: a worker re-arms through WorkManager's own queries, which must not share
        // the worker's thread (a SynchronousExecutor would deadlock there; production does not).
        WorkManagerTestInitHelper.initializeTestWorkManager(
            app,
            Configuration.Builder()
                .setExecutor(Executors.newSingleThreadExecutor())
                .setTaskExecutor(Executors.newSingleThreadExecutor())
                .build(),
        )
        work = WorkManager.getInstance(app)
        scheduler = BackupScheduler(app)
        app.fake.onAfter = { scheduler.rearm(settings) }
    }

    private fun armed(): WorkInfo? = work.getWorkInfosForUniqueWork(BackupScheduler.ON_NEW_MEDIA).get()
        .singleOrNull { it.state == WorkInfo.State.ENQUEUED }

    /** Meets the trigger's constraints and waits until its run has finished. */
    private fun fire(info: WorkInfo) {
        checkNotNull(WorkManagerTestInitHelper.getTestDriver(app)).setAllConstraintsMet(info.id)
        val deadline = System.currentTimeMillis() + 10_000
        while (work.getWorkInfoById(info.id).get()?.state?.isFinished != true) {
            check(System.currentTimeMillis() < deadline) { "the triggered run did not finish" }
            ShadowLooper.idleMainLooper()
            Thread.sleep(20)
        }
        Thread.sleep(100) // the appended trigger becomes ENQUEUED once its prerequisite finished
    }

    @Test
    fun aTriggeredRunReArmsTheTrigger() {
        scheduler.apply(settings)
        val first = checkNotNull(armed())

        fire(first)

        val finished = work.getWorkInfoById(first.id).get()
        assertEquals(WorkInfo.State.SUCCEEDED, finished?.state)
        val next = checkNotNull(armed()) { "the trigger is not armed after its run" }
        assertNotEquals(first.id, next.id)
        assertEquals(2, next.constraints.contentUriTriggers.size)

        fire(next) // and again: the chain does not run dry
        assertNotEquals(next.id, checkNotNull(armed()).id)
    }

    @Test
    fun aColdStartArmsTheTriggerWhenNothingIsPending() {
        scheduler.apply(settings)
        val first = checkNotNull(armed())
        app.fake.failure = IllegalStateException("bug") // the run fails ...
        app.fake.onAfter = {} // ... and does not re-arm
        fire(first)
        assertEquals(null, armed())

        scheduler.apply(settings) // the next time the app's UI starts

        checkNotNull(armed())
    }
}
