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
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
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
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
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
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.IntSize
import app.squirrel.api.models.AssetDetails
import app.squirrel.api.models.AssetSummary
import app.squirrel.api.models.ThumbnailStatus
import app.squirrel.shared.api.PreviewSize
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
import app.squirrel.ui.images.Thumbnail
import app.squirrel.ui.shell.FullScreen
import app.squirrel.ui.shell.StageBars
import app.squirrel.ui.shell.SystemBack
import androidx.compose.ui.input.pointer.PointerInputScope
import kotlinx.coroutines.CancellationException

private const val MAX_ZOOM = 4f
private const val DOUBLE_TAP_ZOOM = 2.5f
private const val PREFETCHED_PAGES = 1

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
    saver: PhotoSaver?,
    items: List<AssetSummary>,
    startId: String,
    onNear: (String) -> Unit,
    onClose: () -> Unit,
    /** More stage buttons for the photo on screen (comments, share…), before "Details". */
    tools: @Composable (AssetSummary) -> Unit = {},
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
    val downloads = rememberPhotoDownloads(api, saver)
    val download = downloads?.state?.collectAsState()?.value ?: DownloadState()
    FullScreen {
        SystemBack(onBack = onClose)
        StageBars()
        // The graphite stage runs under the system bars; it takes every touch the photo leaves.
        BoxWithConstraints(Modifier.fillMaxSize().background(scheme.rail.brush()).pointerInput(Unit) { blockTouches() }) {
            // The stage in device pixels: what a preview has to fill.
            val stage = IntSize(constraints.maxWidth, constraints.maxHeight)
            // One page each side is composed ahead, so its preview is loading before the swipe.
            HorizontalPager(
                pager,
                userScrollEnabled = !zoomed,
                key = { items[it].id },
                beyondViewportPageCount = PREFETCHED_PAGES,
                modifier = Modifier.fillMaxSize(),
            ) { page ->
                ZoomablePhoto(api, items[page], stage, onZoom = { if (page == pager.currentPage) zoomed = it })
            }
            val current = items.getOrNull(pager.currentPage) ?: items.last()
            // The buttons first, then the date on its own line: the tools leave it no room beside them.
            Column(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(Tokens.space3)) {
                Row(horizontalArrangement = Arrangement.spacedBy(Tokens.space1), verticalAlignment = Alignment.CenterVertically) {
                    StageButton(SquirrelIcon.X, "Close", onClose)
                    Spacer(Modifier.weight(1f))
                    if (downloads != null) {
                        StageButton(SquirrelIcon.Download, "Download ${current.name}") { downloads.download(listOf(current.id)) }
                    }
                    tools(current)
                    StageButton(SquirrelIcon.Info, "Details of ${current.name}") { info = current }
                }
                val position = "${pager.currentPage.coerceAtMost(items.lastIndex) + 1} of ${items.size}"
                BasicText(
                    dayLabel(current.takenAt),
                    style = SquirrelTheme.type.label.copy(color = scheme.railInk).tabular(),
                    maxLines = 1,
                    modifier = Modifier.padding(start = Tokens.space3).semantics {
                        // Read on every page turn: "Photo, 12 October 2026, 19:59, 3 of 120".
                        contentDescription = "${photoLabel(current)}, $position"
                        liveRegion = LiveRegionMode.Polite
                    },
                )
            }
            Box(Modifier.align(Alignment.BottomCenter).fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(Tokens.gutter)) {
                download.progress?.let { Progress(it, label = download.progressLabel.orEmpty()) }
                download.message?.let {
                    BasicText(
                        it,
                        style = SquirrelTheme.type.caption.copy(color = scheme.railInk),
                        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                    )
                }
            }
        }
        info?.let { PhotoInfo(api, it) { info = null } }
        DownloadConfirm(downloads)
    }
}

/** Keeps touches on the stage from reaching the shell underneath. */
private suspend fun PointerInputScope.blockTouches() {
    awaitPointerEventScope {
        while (true) awaitPointerEvent().changes.forEach { it.consume() }
    }
}

@Composable
internal fun StageButton(icon: SquirrelIcon, label: String, onClick: () -> Unit) {
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
private fun ZoomablePhoto(api: SquirrelApi, item: AssetSummary, screen: IntSize, onZoom: (Boolean) -> Unit) {
    var scale by remember { mutableFloatStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    var stage by remember { mutableStateOf(IntSize.Zero) }
    LaunchedEffect(scale > 1f) { onZoom(scale > 1f) }
    val ready = item.thumbnail.state == ThumbnailStatus.ready
    val px = PreviewSize.px(item.width, item.height, screen.width.toDouble(), screen.height.toDouble(), density = 1.0)
    Thumbnail(
        url = if (ready) api.thumbnailUrl(item.id, item.thumbnail.version, preview = true, px = px) else null,
        blurhash = item.blurhash,
        contentDescription = photoLabel(item),
        unsupported = !ready && item.thumbnail.state != ThumbnailStatus.pending,
        fit = true,
        modifier = Modifier
            .fillMaxSize()
            .onSizeChanged { stage = it }
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
                            offset = if (scale == 1f) Offset.Zero else clampPan(offset + event.calculatePan(), scale, stage.width.toFloat(), stage.height.toFloat())
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
