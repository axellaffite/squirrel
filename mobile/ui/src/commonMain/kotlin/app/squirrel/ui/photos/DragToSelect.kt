package app.squirrel.ui.photos

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.lazy.LazyListLayoutInfo
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.AwaitPointerEventScope
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.dp

/** How close to the list's top or bottom a sliding finger starts scrolling it. */
private val EDGE_ZONE = 72.dp

/** The most a sliding finger deep in an edge zone scrolls the list per frame. */
private val EDGE_STEP = 18.dp

/** Where the slide is: the finger and the selection it extends; not Compose state, read per frame. */
private class Slide {
    var selection: DragSelection? = null
    var finger: Offset = Offset.Zero
    var active by mutableStateOf(false)
}

private fun LazyListLayoutInfo.rowSpans(): List<RowSpan> =
    visibleItemsInfo.map { RowSpan(it.key, it.offset - viewportStartOffset, it.offset - viewportStartOffset + it.size) }

/**
 * Long press a photo, then slide: the photos between it and the finger are selected (see
 * [DragSelection]), and the list scrolls when the finger nears its top or bottom. For the
 * LazyColumn of [list]; [order] lists the photos shown in reading order, [tilesOf] a row's tiles
 * by item key. Taps and scrolling stay the list's own; a grid without selection gets no gesture.
 */
@Composable
fun rememberDragToSelect(list: LazyListState, actions: TileActions, order: () -> List<String>, tilesOf: (Any) -> List<TileSlot>?): Modifier {
    val latest by rememberUpdatedState(actions)
    val latestOrder by rememberUpdatedState(order)
    val latestTiles by rememberUpdatedState(tilesOf)
    val haptics = LocalHapticFeedback.current
    val density = LocalDensity.current
    val slide = remember { Slide() }

    fun extend() {
        val selection = slide.selection ?: return
        val id = tileAt(list.layoutInfo.rowSpans(), slide.finger.x, slide.finger.y, density.density, latestTiles) ?: return
        val next = selection.over(latestOrder(), id)
        if (next != latest.selected) latest.onSelect?.invoke(next)
    }

    LaunchedEffect(slide.active) {
        if (!slide.active) return@LaunchedEffect
        val zone = with(density) { EDGE_ZONE.toPx() }
        val step = with(density) { EDGE_STEP.toPx() }
        while (true) {
            withFrameNanos {}
            val info = list.layoutInfo
            val bottom = (info.viewportSize.height - info.afterContentPadding).toFloat()
            val by = edgeScroll(slide.finger.y, top = 0f, bottom = bottom, zone = zone, maxStep = step)
            if (by != 0f) {
                list.scrollBy(by)
                extend()
            }
        }
    }

    if (actions.onSelect == null) return Modifier
    return Modifier.pointerInput(list, density) {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            if (!awaitLongPress(down)) return@awaitEachGesture
            val anchor = tileAt(list.layoutInfo.rowSpans(), down.position.x, down.position.y, density.density, latestTiles)
                ?: return@awaitEachGesture
            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
            slide.selection = DragSelection.start(latest.selected, anchor)
            slide.finger = down.position
            slide.active = true
            extend()
            try {
                followFinger(down) { position ->
                    slide.finger = position
                    extend()
                }
            } finally {
                slide.active = false
                slide.selection = null
            }
        }
    }
}

/**
 * True once [down] is held still for the long press timeout; false when it lifts, moves past
 * the touch slop (a scroll), a second finger lands, or something else takes it. Only watches.
 */
private suspend fun AwaitPointerEventScope.awaitLongPress(down: PointerInputChange): Boolean {
    val ended = withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) {
        while (true) {
            val event = awaitPointerEvent(PointerEventPass.Initial)
            val change = event.changes.firstOrNull { it.id == down.id }
            val moved = change != null && (change.position - down.position).getDistance() > viewConfiguration.touchSlop
            if (change == null || !change.pressed || change.isConsumed || moved || event.changes.count { it.pressed } > 1) break
        }
        true
    }
    return ended == null
}

/**
 * Takes every event until [down] lifts, before the list and the tiles see them: no scroll, and
 * no tap on the tile under the finger when it lifts. [onMove] hears each new finger position.
 */
private suspend fun AwaitPointerEventScope.followFinger(down: PointerInputChange, onMove: (Offset) -> Unit) {
    while (true) {
        val event = awaitPointerEvent(PointerEventPass.Initial)
        val change = event.changes.firstOrNull { it.id == down.id } ?: return
        event.changes.forEach { it.consume() }
        if (!change.pressed) return
        onMove(change.position)
    }
}
