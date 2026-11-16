package app.squirrel.android

import android.app.Application
import app.squirrel.ui.AppGraph

class SquirrelApp : Application() {
    /** One graph per process, shared by the activity and (Milestone F) the workers. */
    val graph: AppGraph by lazy { AndroidGraph.create(this) }
}
