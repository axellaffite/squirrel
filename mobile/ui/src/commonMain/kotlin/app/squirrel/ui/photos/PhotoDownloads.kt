package app.squirrel.ui.photos

import app.squirrel.api.models.ArchiveTicket
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.CreateArchiveRequest
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.sync.formatBytes
import app.squirrel.ui.files.CONFIRM_BYTES
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

const val TOO_LARGE_FOR_ZIP = "Too much to put in one zip: select fewer photos."
private const val PAYLOAD_TOO_LARGE = 413

/** A download above [CONFIRM_BYTES], waiting for "Download 1.4 GB?". */
sealed interface PendingDownload {
    val bytes: Long

    data class Original(val asset: AssetSummary, override val bytes: Long) : PendingDownload

    data class Archive(val ticket: ArchiveTicket) : PendingDownload {
        override val bytes: Long get() = ticket.propertySize
    }
}

data class DownloadState(
    /** How far the download is (0–1); null when none runs. */
    val progress: Float? = null,
    /** What runs, for the progress bar ("Downloading the original…"). */
    val progressLabel: String? = null,
    val confirm: PendingDownload? = null,
    /** What the last download did ("Saved to Pictures."), or why it failed. */
    val message: String? = null,
    val failed: Boolean = false,
) {
    val confirmText: String? get() = confirm?.let { "Download ${formatBytes(it.bytes)}?" }

    val confirmDetail: String? get() = when (val pending = confirm) {
        is PendingDownload.Original -> "${pending.asset.name} is large: it takes a while to download."
        is PendingDownload.Archive -> "${photoCount(pending.ticket.count)} make one large zip."
        null -> null
    }
}

/** The Download button's accessible name: "Download IMG_1.jpg" for one photo, else "Download 3 photos". */
fun downloadLabel(count: Int, name: String?): String =
    if (count == 1 && name != null) "Download $name" else "Download ${photoCount(count.toLong())}"

private fun photoCount(count: Long) = if (count == 1L) "1 photo" else "$count photos"

/**
 * Download, for the viewer and the selection bar: one photo's original goes to the phone's photo
 * library, several go into one zip (`POST /api/files/archive`, then its ticket's URL). Above
 * [CONFIRM_BYTES] it asks first. One download at a time; the selection is the screen's business.
 */
class PhotoDownloads(private val api: SquirrelApi, private val saver: PhotoSaver, private val scope: CoroutineScope) {
    private val _state = MutableStateFlow(DownloadState())
    val state: StateFlow<DownloadState> = _state.asStateFlow()

    fun download(ids: List<String>) {
        if (ids.isEmpty() || _state.value.progress != null) return
        val label = if (ids.size == 1) ORIGINAL else "Downloading ${photoCount(ids.size.toLong())}…"
        _state.update { DownloadState(progress = 0f, progressLabel = label) }
        scope.launch {
            val pending = try {
                if (ids.size == 1) original(ids.single()) else archive(ids)
            } catch (e: CancellationException) {
                if (!currentCoroutineContext().isActive) throw e
                return@launch fail(UNREACHABLE)
            } catch (e: ApiError) {
                return@launch fail(problem(e))
            } catch (_: Exception) {
                return@launch fail(UNREACHABLE)
            }
            if (pending.bytes > CONFIRM_BYTES) {
                _state.update { DownloadState(confirm = pending) }
            } else {
                save(pending)
            }
        }
    }

    fun confirm() {
        val pending = _state.value.confirm ?: return
        _state.update { DownloadState(progress = 0f, progressLabel = labelOf(pending)) }
        scope.launch { save(pending) }
    }

    fun dismiss() = _state.update { it.copy(confirm = null) }

    private suspend fun original(id: String): PendingDownload {
        val details = api.photos.getPhoto(id).bodyOrThrow()
        return PendingDownload.Original(details.asset, details.propertySize)
    }

    private suspend fun archive(ids: List<String>): PendingDownload =
        PendingDownload.Archive(api.files.createArchive(CreateArchiveRequest(ids)).bodyOrThrow())

    private suspend fun save(pending: PendingDownload) {
        val onProgress: (Float) -> Unit = { p -> _state.update { if (it.progress != null) it.copy(progress = p) else it } }
        val result = try {
            when (pending) {
                is PendingDownload.Original -> saver.saveOriginal(api, pending.asset.id, pending.asset.name, pending.asset.mime, onProgress)
                is PendingDownload.Archive -> saver.saveArchive(api, api.archiveUrl(pending.ticket.ticket), pending.ticket.name, onProgress)
            }
        } finally {
            _state.update { it.copy(progress = null, progressLabel = null) }
        }
        when (result) {
            is SaveResult.Saved -> _state.update { it.copy(message = "Saved to ${result.place}.", failed = false) }
            SaveResult.Handed -> _state.update { it.copy(message = null, failed = false) }
            is SaveResult.Failed -> fail(result.message)
        }
    }

    private fun fail(message: String) = _state.update { DownloadState(message = message, failed = true) }

    private fun labelOf(pending: PendingDownload) = when (pending) {
        is PendingDownload.Original -> ORIGINAL
        is PendingDownload.Archive -> "Downloading ${photoCount(pending.ticket.count)}…"
    }

    private fun problem(e: ApiError): String = when {
        e.status == PAYLOAD_TOO_LARGE -> TOO_LARGE_FOR_ZIP
        e.fromServer -> e.message ?: UNREACHABLE
        else -> UNREACHABLE
    }

    private companion object {
        const val ORIGINAL = "Downloading the original…"
    }
}
