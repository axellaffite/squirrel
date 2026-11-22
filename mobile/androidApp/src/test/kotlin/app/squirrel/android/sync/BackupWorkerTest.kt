package app.squirrel.android.sync

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.work.ListenableWorker
import androidx.work.testing.TestListenableWorkerBuilder
import app.squirrel.shared.sync.SyncState
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull

/** The worker's Application, with a runner the test controls. */
class FakeBackupApp : Application(), BackupHost {
    val fake = FakeRunner()
    override val backupRunner: BackupRunner get() = fake
}

class FakeRunner : BackupRunner {
    var outcome: RunOutcome = RunOutcome.Done
    var failure: Exception? = null
    var pending = 0L to 0L
    var deadline: Long? = -1
    var after: RunOutcome? = null
    var afterCalled = false
    var onAfter: suspend () -> Unit = {}

    override suspend fun pending() = pending
    override val progress: Flow<SyncState> = flowOf(SyncState.Idle)
    var manual: Boolean? = null

    var reconcile: Boolean? = null

    var waitForNetwork: Boolean? = null

    override suspend fun run(deadline: Long?, manual: Boolean, reconcile: Boolean, waitForNetwork: Boolean): RunOutcome {
        this.reconcile = reconcile
        this.waitForNetwork = waitForNetwork
        this.deadline = deadline
        this.manual = manual
        failure?.let { throw it }
        return outcome
    }
    override suspend fun afterRun(outcome: RunOutcome?, manual: Boolean) {
        after = outcome
        afterCalled = true
        onAfter()
    }
}

@RunWith(AndroidJUnit4::class)
@Config(sdk = [35], application = FakeBackupApp::class)
class BackupWorkerTest {
    private val app = ApplicationProvider.getApplicationContext<FakeBackupApp>()

    private fun run(): ListenableWorker.Result = runBlocking {
        TestListenableWorkerBuilder<BackupWorker>(app).build().doWork()
    }

    @Test
    fun aFinishedOrPausedRunSucceeds() {
        app.fake.outcome = RunOutcome.Done
        assertEquals(ListenableWorker.Result.success(), run())
        assertEquals(RunOutcome.Done, app.fake.after)
    }

    @Test
    fun anUnreachableServerIsRetried() {
        app.fake.outcome = RunOutcome.Retry
        assertEquals(ListenableWorker.Result.retry(), run())
    }

    @Test
    fun aDeadlineStopSucceedsAndAsksToContinue() {
        app.fake.outcome = RunOutcome.Continue
        assertEquals(ListenableWorker.Result.success(), run())
        assertEquals(RunOutcome.Continue, app.fake.after)
    }

    @Test
    fun aSmallBacklogRunsWithTheNineMinuteDeadline() {
        app.fake.pending = 3L to 1_000L
        run()
        assertNotNull(app.fake.deadline)
        assertEquals(false, app.fake.waitForNetwork) // a deadline run ends on a network loss
    }

    @Test
    fun aSystemStopContinuesUnlessTheAppCancelled() {
        assertEquals(true, continuesAfterStop(androidx.work.WorkInfo.STOP_REASON_DEVICE_STATE))
        assertEquals(true, continuesAfterStop(androidx.work.WorkInfo.STOP_REASON_FOREGROUND_SERVICE_TIMEOUT))
        assertEquals(true, continuesAfterStop(androidx.work.WorkInfo.STOP_REASON_CONSTRAINT_CONNECTIVITY))
        assertEquals(false, continuesAfterStop(androidx.work.WorkInfo.STOP_REASON_CANCELLED_BY_APP))
        assertEquals(false, continuesAfterStop(androidx.work.WorkInfo.STOP_REASON_NOT_STOPPED))
    }

    @Test
    fun aLargeBacklogRunsInTheForegroundWithoutDeadline() {
        app.fake.pending = 500L to 1_000_000_000L
        run()
        assertNull(app.fake.deadline)
        assertEquals(true, app.fake.waitForNetwork) // and waits out a network loss
    }

    @Test
    fun anUnexpectedFailureFailsButStillRearms() {
        app.fake.failure = IllegalStateException("bug")
        assertEquals(ListenableWorker.Result.failure(), run())
        assertEquals(true, app.fake.afterCalled)
    }

    @Test
    fun backUpNowRunsAsManual() {
        runBlocking {
            TestListenableWorkerBuilder<BackupWorker>(app)
                .setInputData(androidx.work.Data.Builder().putBoolean(BackupWorker.KEY_MANUAL, true).build())
                .build().doWork()
        }
        assertEquals(true, app.fake.manual)
        run()
        assertEquals(false, app.fake.manual)
    }

    @Test
    fun backUpNowRunsInTheForegroundEvenWhenSmall() {
        app.fake.pending = 1L to 10L
        runBlocking {
            TestListenableWorkerBuilder<BackupWorker>(app)
                .setInputData(androidx.work.Data.Builder().putBoolean(BackupWorker.KEY_MANUAL, true).build())
                .build().doWork()
        }
        assertNull(app.fake.deadline) // foreground: no 9-minute deadline
    }

    @Test
    fun foregroundRefusalsAreRecognised() {
        kotlin.test.assertTrue(isForegroundRefusal(IllegalStateException("ForegroundServiceStartNotAllowedException")))
        kotlin.test.assertTrue(isForegroundRefusal(SecurityException("missing FOREGROUND_SERVICE_DATA_SYNC")))
        kotlin.test.assertFalse(isForegroundRefusal(IllegalArgumentException()))
    }

    @Test
    fun onlyThePeriodicRunReconciles() {
        runBlocking { TestListenableWorkerBuilder<BackupWorker>(app).setTags(listOf(BackupScheduler.PERIODIC_TAG)).build().doWork() }
        assertEquals(true, app.fake.reconcile)
        run()
        assertEquals(false, app.fake.reconcile)
    }
}
