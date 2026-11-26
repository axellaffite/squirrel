package app.squirrel.ui.photos

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.Progress
import app.squirrel.theme.components.Sheet
import app.squirrel.theme.popShadow

/** A screen's [PhotoDownloads], alive as long as the screen; null where the phone cannot save. */
@Composable
fun rememberPhotoDownloads(api: SquirrelApi, saver: PhotoSaver?): PhotoDownloads? {
    val scope = rememberCoroutineScope()
    return remember(api, saver) { saver?.let { PhotoDownloads(api, it, scope) } }
}

/** The "Download 1.4 GB?" sheet of [downloads], when one is waiting. */
@Composable
fun DownloadConfirm(downloads: PhotoDownloads?) {
    val target = downloads ?: return
    val state by target.state.collectAsState()
    if (state.confirm == null) return
    Sheet(state.confirmText.orEmpty(), onDismiss = target::dismiss) {
        BasicText(state.confirmDetail.orEmpty(), style = SquirrelTheme.type.body)
        Button("Download", onClick = target::confirm, full = true)
        Button("Cancel", onClick = target::dismiss, variant = ButtonVariant.Quiet, full = true)
    }
}

/** The selection bar's Download: "Download IMG_1.jpg" for one photo, "Download 3 photos" for several. */
@Composable
fun DownloadAction(downloads: PhotoDownloads, selected: Set<String>, nameOf: (String) -> String?) {
    val state by downloads.state.collectAsState()
    IconButton(
        SquirrelIcon.Download,
        label = downloadLabel(selected.size, selected.singleOrNull()?.let(nameOf)),
        onClick = { downloads.download(selected.toList()) },
        enabled = state.progress == null,
    )
}

/** Above the selection bar: the download's progress, then what it did; nothing otherwise. */
@Composable
fun DownloadStatus(downloads: PhotoDownloads?) {
    val target = downloads ?: return
    val state by target.state.collectAsState()
    val progress = state.progress
    val message = state.message
    if (progress == null && message == null) return
    val scheme = SquirrelTheme.scheme
    Box(
        Modifier
            .fillMaxWidth()
            .popShadow(Shapes.pill)
            .background(scheme.card, Shapes.pill)
            .padding(horizontal = Tokens.space4, vertical = Tokens.space2),
    ) {
        when {
            progress != null -> Progress(progress, label = state.progressLabel.orEmpty())
            message != null -> BasicText(
                message,
                style = SquirrelTheme.type.caption.copy(color = if (state.failed) scheme.danger else scheme.ink),
                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            )
        }
    }
}
