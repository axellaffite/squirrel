package app.squirrel.ui.photos

import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import app.squirrel.api.models.AlbumDto
import app.squirrel.api.models.ThumbnailStatus
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.Sheet
import app.squirrel.ui.attempt
import app.squirrel.ui.explore.photoCount
import app.squirrel.ui.files.albumNameProblem
import app.squirrel.ui.images.Thumbnail
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.memberName
import app.squirrel.ui.shell.NameSheet
import kotlinx.coroutines.launch

/**
 * MASTER "Album picker": "New album…" first, then one's own albums and those shared for writing,
 * by name. [onDone] hears what happened ("Added 3 photos to Holidays.") once the photos are in.
 */
@Composable
fun AlbumPickerSheet(api: SquirrelApi, ids: List<String>, onDone: (String) -> Unit, onClose: () -> Unit) {
    // Kept across retries: a retried "New album…" fills the album already created.
    val adder = remember(ids) { AlbumAdder(api, ids) }
    val scope = rememberCoroutineScope()
    var naming by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val albums by produceState<List<AlbumDto>?>(null) {
        val own = attempt { api.albums.listAlbums().bodyOrThrow() }
        val shared = attempt { api.sharing.sharedWithMe().bodyOrThrow() }.orEmpty()
        if (own == null) error = app.squirrel.ui.photos.UNREACHABLE
        value = writableAlbums(own.orEmpty(), shared)
    }
    if (naming) {
        NameSheet("New album", "Create album", validate = ::albumNameProblem, onSubmit = { name ->
            when (val result = adder.intoNew(name)) {
                is Outcome.Ok -> null.also { onDone(result.value) }
                is Outcome.Failed -> result.message
            }
        }, onClose = onClose)
        return
    }
    Sheet("Add to album", onDismiss = onClose) {
        error?.let { Alert(it, tone = AlertTone.Error) }
        ListRow("New album…", icon = SquirrelIcon.Plus, onClick = { if (!busy) naming = true })
        albums?.forEach { album ->
            ListRow(
                name = album.name,
                meta = listOfNotNull(photoCount(album.itemCount), album.sharedBy?.let(::memberName)).joinToString(" · "),
                onClick = {
                    if (!busy) {
                        busy = true
                        error = null
                        scope.launch {
                            when (val result = adder.into(album)) {
                                is Outcome.Ok -> {
                                    onDone(result.value)
                                    onClose()
                                }
                                is Outcome.Failed -> error = result.message
                            }
                            busy = false
                        }
                    }
                },
                leading = {
                    val cover = album.cover
                    Thumbnail(
                        url = cover?.takeIf { it.thumbnail.state == ThumbnailStatus.ready }?.let { api.thumbnailUrl(it.id, it.thumbnail.version) },
                        blurhash = cover?.blurhash,
                        contentDescription = null,
                        modifier = Modifier.size(Tokens.wellMd).clip(Shapes.circle),
                    )
                },
            )
        } ?: if (error == null) Loading() else Unit
    }
}
