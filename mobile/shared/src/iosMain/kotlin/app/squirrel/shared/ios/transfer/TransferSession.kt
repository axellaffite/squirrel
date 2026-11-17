package app.squirrel.shared.ios.transfer

import kotlinx.cinterop.BetaInteropApi
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import platform.Foundation.NSFileManager
import platform.Foundation.NSURL
import platform.Foundation.NSURLRequest
import platform.Foundation.NSURLSession
import platform.Foundation.NSURLSessionConfiguration
import platform.Foundation.NSURLSessionTask
import kotlin.coroutines.resume

/** The background session's identifier: one per app, recreated with the same id after a relaunch. */
const val UPLOAD_SESSION_ID = "app.squirrel.upload"

/**
 * One `NSURLSession` uploading chunk files. A chunk is a task named "<upload_id>:<index>"; its
 * result goes to [store] first (it may finish while nobody waits), then to whoever waits for it.
 * Waiting is cancellable; the task itself is not cancelled: it carries on in the background.
 */
@OptIn(ExperimentalForeignApi::class, BetaInteropApi::class)
class TransferSession(
    configuration: NSURLSessionConfiguration,
    private val store: TransportStore,
    private val chunkDir: String,
) {
    private val lock = Mutex()
    private val waiters = mutableMapOf<String, CompletableDeferred<ChunkResult>>()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /** Called once the system delivered every event for a background relaunch. */
    var onEventsFinished: () -> Unit = {}

    private val delegate = TransportDelegate(
        onComplete = { task, result ->
            store.record(task, result)
            NSFileManager.defaultManager.removeItemAtPath(chunkPath(task), null)
            scope.launch { lock.withLock { waiters.remove(task) }?.complete(result) }
        },
        onEventsFinished = { onEventsFinished() },
    )
    private val session = NSURLSession.sessionWithConfiguration(configuration, delegate, delegateQueue = null)

    /** Where the bytes of [task] wait for their upload. */
    fun chunkPath(task: String): String = "$chunkDir/${task.replace(':', '-')}"

    /**
     * Uploads [request] with the body in [chunkPath]([task]) (written by [write] unless the task
     * is already running, e.g. since before a relaunch), and waits for its result.
     */
    suspend fun send(task: String, request: NSURLRequest, write: suspend (path: String) -> Unit): ChunkResult {
        store.take(task)?.let { return it } // finished while nobody was waiting
        val (waiter, start) = lock.withLock {
            val existing = waiters[task]
            if (existing != null) existing to false
            else CompletableDeferred<ChunkResult>().also { waiters[task] = it } to true
        }
        if (start && task !in running()) {
            NSFileManager.defaultManager.createDirectoryAtPath(chunkDir, withIntermediateDirectories = true, attributes = null, error = null)
            val path = chunkPath(task)
            write(path)
            val upload = session.uploadTaskWithRequest(request, fromFile = NSURL.fileURLWithPath(path))
            upload.taskDescription = task
            upload.resume()
        }
        val result = waiter.await()
        store.take(task) // consumed here: never seen twice
        return result
    }

    /** Names of the tasks the system is still running for this session. */
    private suspend fun running(): Set<String> = suspendCancellableCoroutine { continuation ->
        session.getAllTasksWithCompletionHandler { tasks ->
            continuation.resume(tasks.orEmpty().mapNotNullTo(HashSet()) { (it as NSURLSessionTask).taskDescription })
        }
    }

    /** Signed out: stops every transfer and forgets the chunk files. */
    fun cancelAll() {
        session.getAllTasksWithCompletionHandler { tasks -> tasks.orEmpty().forEach { (it as NSURLSessionTask).cancel() } }
        NSFileManager.defaultManager.removeItemAtPath(chunkDir, null)
    }
}
