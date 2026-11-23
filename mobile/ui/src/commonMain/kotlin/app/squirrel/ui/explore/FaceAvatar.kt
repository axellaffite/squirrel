package app.squirrel.ui.explore

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import app.squirrel.api.models.FaceCrop
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Icon
import app.squirrel.ui.images.LocalImageLoader
import coil3.compose.AsyncImage
import kotlin.math.roundToInt

/**
 * MASTER.md "Face avatar": a circle on `--tint`, a client-side crop of the photo's thumbnail to
 * the face box (no face image exists on the server). While loading, or without a face, the
 * `user` icon in `--ink-muted`.
 */
@Composable
fun FaceAvatar(api: SquirrelApi, face: FaceCrop?, size: Dp = Tokens.faceSm, modifier: Modifier = Modifier) {
    val scheme = SquirrelTheme.scheme
    val density = LocalDensity.current
    val sizePx = with(density) { size.toPx() }
    // The photo's width / height, once its thumbnail loaded: the crop stays inside it.
    var aspect by remember(face?.faceId) { mutableStateOf(1.0) }
    var failed by remember(face?.faceId) { mutableStateOf(false) }
    val loader = LocalImageLoader.current
    Box(modifier.size(size).clip(Shapes.circle).background(scheme.tint), contentAlignment = Alignment.Center) {
        Icon(SquirrelIcon.User, contentDescription = null, tint = scheme.inkMuted)
        val source = face?.let { cropSource(api, it, sizePx.toDouble(), aspect) }
        if (face != null && source != null && loader != null && !failed) {
            val crop = cropWindow(face.box, aspect)
            val imageHeight = sizePx / crop.height
            val imageWidth = imageHeight * aspect
            AsyncImage(
                model = source,
                imageLoader = loader,
                contentDescription = null,
                contentScale = ContentScale.FillBounds,
                onSuccess = { state ->
                    val image = state.result.image
                    if (image.width > 0 && image.height > 0) aspect = image.width.toDouble() / image.height
                },
                onError = { failed = true },
                modifier = Modifier.layout { measurable, constraints ->
                    val w = imageWidth.roundToInt().coerceAtLeast(1)
                    val h = imageHeight.roundToInt().coerceAtLeast(1)
                    val placeable = measurable.measure(Constraints.fixed(w, h))
                    layout(constraints.maxWidth, constraints.maxHeight) {
                        placeable.place(
                            x = (sizePx / 2 - crop.centre * imageWidth).roundToInt(),
                            y = (-crop.top * imageHeight).roundToInt(),
                        )
                    }
                },
            )
        }
    }
}
