package app.squirrel.shared.ios.transfer

import app.squirrel.shared.db.SquirrelDatabase

/** How a chunk transfer ended: an HTTP [status] (0 when no answer came) and the failure, if any. */
data class ChunkResult(
    val status: Int,
    val error: String?,
    /** Cancelled on purpose (sign-out): never worth keeping. */
    val cancelled: Boolean = false,
) {
    val succeeded: Boolean get() = status in 200..299
}

/** Results last a day: a chunk not asked for again by then is resent anyway. */
const val RESULT_KEEP_MS = 24L * 60 * 60 * 1000

/**
 * Chunk results as the session delegate reports them, kept in the database so a result that
 * arrives while the app is suspended (or relaunched in the background) is not lost. Each one is
 * consumed exactly once.
 */
class TransportStore(private val db: SquirrelDatabase, private val now: () -> Long) {
    fun record(task: String, result: ChunkResult) {
        db.transferResultQueries.record(task, result.status.toLong(), result.error, now())
    }

    /** The result of [task], removed as it is read; null when none was recorded. */
    fun take(task: String): ChunkResult? = db.transactionWithResult {
        val row = db.transferResultQueries.byTask(task).executeAsOneOrNull() ?: return@transactionWithResult null
        db.transferResultQueries.forget(task)
        ChunkResult(row.status.toInt(), row.error)
    }

    fun prune() = db.transferResultQueries.pruneBefore(now() - RESULT_KEEP_MS)
}
