package app.squirrel.ui.photos

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import app.squirrel.api.models.AssetDetails
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.ThumbnailStatus
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Icon
import app.squirrel.theme.components.MinTouchTarget
import app.squirrel.theme.components.Progress
import app.squirrel.theme.components.Sheet
import app.squirrel.theme.tabular
import app.squirrel.ui.files.FileHandoff
import app.squirrel.ui.files.HandoffResult
import app.squirrel.ui.images.Thumbnail
import app.squirrel.ui.shell.FullScreen
import app.squirrel.ui.shell.StageBars
import app.squirrel.ui.shell.SystemBack
import androidx.compose.ui.input.pointer.PointerInputScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

private const val MAX_ZOOM = 4f
private const val DOUBLE_TAP_ZOOM = 2.5f

/**
 * MASTER.md lightbox: a full-screen graphite stage (`--rail`), the photo's `preview` variant,
 * swipe between the loaded photos, controls as `--rail-well` circles with `--rail-ink` icons.
 */
/**
 * Where the viewer stands in [ids] after they changed: [anchor]'s new place, or (when it left
 * the list) [current] clamped to what is left.
 */
fun viewerIndex(ids: List<String>, anchor: String, current: Int): Int {
    val at = ids.indexOf(anchor)
    return if (at >= 0) at else current.coerceIn(0, (ids.size - 1).coerceAtLeast(0))
}

/**
 * The photo viewer over a live list: [items] grows as pages arrive (it asks for them with
 * [onNear]) and the photo on screen stays put, whatever lands before it.
 */
@Composable
fun PhotoViewer(
    api: SquirrelApi,
    files: FileHandoff?,
    items: List<AssetSummary>,
    startId: String,
    onNear: (String) -> Unit,
    onClose: () -> Unit,
) {
    if (items.isEmpty()) {
        LaunchedEffect(Unit) { onClose() }
        return
    }
    val scheme = SquirrelTheme.scheme
    var anchor by remember { mutableStateOf(startId) }
    val pager = rememberPagerState(initialPage = remember { viewerIndex(items.map { it.id }, startId, 0) }) { items.size }
    val latest by rememberUpdatedState(items)
    val near by rememberUpdatedState(onNear)
    LaunchedEffect(items) {
        val index = viewerIndex(items.map { it.id }, anchor, pager.currentPage)
        if (index != pager.currentPage) pager.scrollToPage(index)
    }
    LaunchedEffect(pager) {
        snapshotFlow { pager.currentPage }.collect { page ->
            latest.getOrNull(page)?.let {
                anchor = it.id
                near(it.id)
            }
        }
    }
    var zoomed by remember { mutableStateOf(false) }
    var info by remember { mutableStateOf<AssetSummary?>(null) }
    var progress by remember { mutableStateOf<Float?>(null) }
    var message by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    FullScreen {
        SystemBack(onBack = onClose)
        StageBars()
        // The graphite stage runs under the system bars; it takes every touch the photo leaves.
        Box(Modifier.fillMaxSize().background(scheme.rail.brush()).pointerInput(Unit) { blockTouches() }) {
            HorizontalPager(pager, userScrollEnabled = !zoomed, key = { items[it].id }, modifier = Modifier.fillMaxSize()) { page ->
                ZoomablePhoto(api, items[page], onZoom = { if (page == pager.currentPage) zoomed = it })
            }
            val current = items.getOrNull(pager.currentPage) ?: items.last()
            Row(
                modifier = Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(Tokens.space3),
                horizontalArrangement = Arrangement.spacedBy(Tokens.space2),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                StageButton(SquirrelIcon.X, "Close", onClose)
                BasicText(
                    dayLabel(current.takenAt),
                    style = SquirrelTheme.type.label.copy(color = scheme.railInk).tabular(),
                    modifier = Modifier.weight(1f),
                )
                if (files != null) {
                    StageButton(SquirrelIcon.ExternalLink, "Open ${current.name} in another app") {
                        if (progress != null) return@StageButton
                        progress = 0f
                        scope.launch {
                            val result = files.open(api, current.id, current.name, current.mime, share = false) { progress = it }
                            progress = null
                            message = when (result) {
                                HandoffResult.Opened -> null
                                HandoffResult.NoApp -> "No app on this phone opens this file."
                                is HandoffResult.Failed -> result.message
                            }
                        }
                    }
                }
                StageButton(SquirrelIcon.Info, "Details of ${current.name}") { info = current }
            }
            Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(Tokens.gutter)) {
                progress?.let { Progress(it, label = "Downloading the original") }
                message?.let { BasicText(it, style = SquirrelTheme.type.caption.copy(color = scheme.railInk)) }
            }
        }
        info?.let { PhotoInfo(api, it) { info = null } }
    }
}

/** Keeps touches on the stage from reaching the shell underneath. */
private suspend fun PointerInputScope.blockTouches() {
    awaitPointerEventScope {
        while (true) awaitPointerEvent().changes.forEach { it.consume() }
    }
}

@Composable
private fun StageButton(icon: SquirrelIcon, label: String, onClick: () -> Unit) {
    val scheme = SquirrelTheme.scheme
    Box(
        modifier = Modifier
            .sizeIn(minWidth = MinTouchTarget, minHeight = MinTouchTarget)
            .clickable(remember { MutableInteractionSource() }, indication = null, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(Tokens.controlSm).background(scheme.railWell, Shapes.circle), contentAlignment = Alignment.Center) {
            Icon(icon, contentDescription = null, tint = scheme.railInk)
        }
    }
}

/** Pinch or double-tap to zoom; one finger pans while zoomed and swipes the pager otherwise. */
@Composable
private fun ZoomablePhoto(api: SquirrelApi, item: AssetSummary, onZoom: (Boolean) -> Unit) {
    var scale by remember { mutableFloatStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    LaunchedEffect(scale > 1f) { onZoom(scale > 1f) }
    val ready = item.thumbnail.state == ThumbnailStatus.ready
    Thumbnail(
        url = if (ready) api.thumbnailUrl(item.id, item.thumbnail.version, preview = true) else null,
        blurhash = item.blurhash,
        contentDescription = "Photo taken ${dayLabel(item.takenAt)}",
        unsupported = !ready && item.thumbnail.state != ThumbnailStatus.pending,
        fit = true,
        modifier = Modifier
            .fillMaxSize()
            .pointerInput(Unit) {
                detectTapGestures(onDoubleTap = {
                    scale = if (scale > 1f) 1f else DOUBLE_TAP_ZOOM
                    offset = Offset.Zero
                })
            }
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false)
                    do {
                        val event = awaitPointerEvent()
                        if (event.changes.size >= 2 || scale > 1f) {
                            scale = (scale * event.calculateZoom()).coerceIn(1f, MAX_ZOOM)
                            offset = if (scale == 1f) Offset.Zero else offset + event.calculatePan()
                            event.changes.forEach { it.consume() }
                        }
                    } while (event.changes.any { it.pressed })
                }
            }
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
                translationX = offset.x
                translationY = offset.y
            },
    )
}

/** Date, place and camera from `GET /api/photos/{id}`. */
@Composable
private fun PhotoInfo(api: SquirrelApi, item: AssetSummary, onDismiss: () -> Unit) {
    var details by remember(item.id) { mutableStateOf<AssetDetails?>(null) }
    var failed by remember(item.id) { mutableStateOf(false) }
    LaunchedEffect(item.id) {
        try {
            details = api.photos.getPhoto(item.id).bodyOrThrow()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            failed = true
        }
    }
    Sheet(item.name, onDismiss = onDismiss) {
        val type = SquirrelTheme.type
        BasicText("${dayLabel(item.takenAt)}, ${item.takenAt.drop(11).take(5)}", style = type.body.tabular())
        val d = details
        when {
            failed -> BasicText(UNREACHABLE, style = type.caption)
            d == null -> BasicText("Loading…", style = type.caption)
            else -> {
                d.place?.let { place ->
                    BasicText(listOfNotNull(place.name, place.admin1, place.country).joinToString(", "), style = type.body)
                }
                d.camera?.let { camera ->
                    val body = listOfNotNull(
                        listOfNotNull(camera.make, camera.model).joinToString(" ").ifBlank { null },
                        camera.lens,
                        camera.fNumber?.let { "f/$it" },
                        camera.exposureTime?.let { "$it s" },
                        camera.iso?.let { "ISO $it" },
                    ).joinToString(" · ")
                    if (body.isNotBlank()) BasicText(body, style = type.caption.tabular())
                }
                BasicText(app.squirrel.shared.sync.formatBytes(d.propertySize), style = type.caption.tabular())
            }
        }
    }
}
