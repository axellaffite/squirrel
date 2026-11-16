package app.squirrel.android

import android.app.Application
import app.squirrel.android.sync.AndroidBackup
import app.squirrel.android.sync.AppBackupRunner
import app.squirrel.android.sync.BackupHost
import app.squirrel.android.sync.BackupRunner
import app.squirrel.ui.AppGraph
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

class SquirrelApp : Application(), BackupHost {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val backup: AndroidBackup by lazy { AndroidBackup(this) }

    /** One graph per process, shared by the activity and the workers. */
    val graph: AppGraph by lazy { AndroidGraph.create(this, backup, scope) }

    override val backupRunner: BackupRunner by lazy { AppBackupRunner(graph, backup, scope) }
}
