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

    override suspend fun pending() = pending
    override val progress: Flow<SyncState> = flowOf(SyncState.Idle)
    override suspend fun run(deadline: Long?): RunOutcome {
        this.deadline = deadline
        failure?.let { throw it }
        return outcome
    }
    override fun afterRun(outcome: RunOutcome?) {
        after = outcome
        afterCalled = true
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
    }

    @Test
    fun aLargeBacklogRunsInTheForegroundWithoutDeadline() {
        app.fake.pending = 500L to 1_000_000_000L
        run()
        assertNull(app.fake.deadline)
    }

    @Test
    fun anUnexpectedFailureFailsButStillRearms() {
        app.fake.failure = IllegalStateException("bug")
        assertEquals(ListenableWorker.Result.failure(), run())
        assertEquals(true, app.fake.afterCalled)
    }
}
