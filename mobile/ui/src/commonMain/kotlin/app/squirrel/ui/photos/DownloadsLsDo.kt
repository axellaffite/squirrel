package app.squirrel.ui.photos

import app.squirrel.api.models.AssetSummary
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.combineState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.Serializable

/** A screen's Download (the selection bar's), as LS-DO shows it. */
@Serializable
data class DownloadView(
    @Doc("How far the download is, 0 to 1; null when none runs") val progress: Float? = null,
    @Doc("A large download waiting for confirm_download or cancel_download, e.g. Download 1.4 GB?") val question: String? = null,
    @Doc("What the last download did, e.g. Saved to Pictures., or why it failed") val message: String? = null,
    @Doc("The message is an error") val failed: Boolean = false,
)

fun DownloadState.view(): DownloadView = DownloadView(progress, confirmText, message, failed)

/** [this] overlay with [downloads]' state in it (the screen's mirror never sets it). */
internal fun StateFlow<PhotosOverlay>.withDownloads(downloads: PhotoDownloads?): StateFlow<PhotosOverlay> =
    if (downloads == null) this else combineState(this, downloads.state) { overlay, state -> overlay.copy(download = state) }

internal const val DOWNLOAD_DOC =
    "Save loaded photos to this phone, as selecting them and choosing Download does: one photo goes to the photo library, " +
        "several to one zip (Downloads on Android, the share sheet on iOS). Above 200 MB it asks first (download.question). " +
        "ls again until download.progress is null"
internal const val CONFIRM_DOWNLOAD_DOC = "Download what download.question asks about, as its Download button does"
internal const val CANCEL_DOWNLOAD_DOC = "Do not download what download.question asks about, as its Cancel button does"

internal fun downloadShown(downloads: PhotoDownloads?, ids: List<String>, shown: List<AssetSummary>, where: String): String? {
    val target = downloads ?: return "Photos cannot be downloaded here."
    missingPhotos(ids, shown, where)?.let { return it }
    if (target.state.value.progress != null) return "A download is already running."
    return done { target.download(ids) }
}

internal fun confirmDownload(downloads: PhotoDownloads?): String? =
    if (downloads?.state?.value?.confirm == null) "No download waits for an answer." else done { downloads.confirm() }

internal fun cancelDownload(downloads: PhotoDownloads?): String? =
    if (downloads?.state?.value?.confirm == null) "No download waits for an answer." else done { downloads.dismiss() }

/** download, or the answers to its question; only where the screen downloads and no photo is open. */
internal fun MutableSet<String>.addDownloadIntents(download: DownloadView?, viewing: String?) {
    if (download == null || viewing != null) return
    when {
        download.question != null -> addAll(listOf("confirm_download", "cancel_download"))
        download.progress == null -> add("download")
    }
}
