package app.squirrel.ui.images

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.layout.ContentScale
import coil3.ImageLoader
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import coil3.compose.AsyncImage
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.components.Icon
import app.squirrel.ui.photos.LocalBlur

/** The signed-in device's image loader (null while signed out). */
val LocalImageLoader = staticCompositionLocalOf<ImageLoader?> { null }

private const val HASH_W = 16
private const val HASH_H = 12
private const val HASH_CACHE = 256
/** The server's `Retry-After` when it is too busy to render a preview. */
private const val PREVIEW_RETRY_MS = 1000L

/** Small decoded previews, bounded: a 16 × 12 bitmap per hash, scaled behind the tile. */
private val previews = LruCache<String, ImageBitmap>(HASH_CACHE)

/** The preview for [hash], decoded off the main thread; null for an invalid hash. */
suspend fun blurhashBitmap(hash: String): ImageBitmap? {
    previews.get(hash)?.let { return it }
    val bitmap = withContext(Dispatchers.Default) {
        if (!Blurhash.isValid(hash)) null else imageBitmapOf(Blurhash.decode(hash, HASH_W, HASH_H), HASH_W, HASH_H)
    } ?: return null
    previews.put(hash, bitmap)
    return bitmap
}

/**
 * MASTER.md photo tile: the blurhash (or `--tint`) while loading, then the image, cropped.
 * [url] null: nothing to load ([unsupported] shows `image-off`, otherwise "preparing").
 * A photo the blur filter hides ([photoId] blurred, see `LocalBlur`, or [blurred]) is never
 * loaded: its blurhash shows under `--photo-shade` with the `eye-off` mark.
 */
@Composable
fun Thumbnail(
    url: String?,
    blurhash: String?,
    contentDescription: String?,
    modifier: Modifier = Modifier,
    unsupported: Boolean = false,
    /** Fit the whole image (the viewer) instead of cropping it to the tile. */
    fit: Boolean = false,
    /** The photo's node id: the blur filter is asked about it. */
    photoId: String? = null,
    /** Shown blurred whatever the filter says (photos a filter being edited matches). */
    blurred: Boolean = false,
) {
    if (!fit && (blurred || LocalBlur.current.isBlurred(photoId))) {
        return BlurredThumbnail(blurhash, contentDescription, modifier)
    }
    val scheme = SquirrelTheme.scheme
    val decoded by produceState<ImageBitmap?>(null, blurhash) { value = blurhash?.let { blurhashBitmap(it) } }
    // A load that failed (offline, deleted, refused) shows `image-off`, not a blurhash forever.
    var failed by remember(url) { mutableStateOf(false) }
    // A viewer preview the server was too busy to render (503/429) is asked for once more.
    var attempt by remember(url) { mutableStateOf(0) }
    var retrying by remember(url) { mutableStateOf(false) }
    LaunchedEffect(retrying) {
        if (retrying) {
            delay(PREVIEW_RETRY_MS)
            attempt += 1
            retrying = false
        }
    }
    val preview = decoded.takeUnless { failed }
    Box(if (fit) modifier else modifier.background(scheme.tint), contentAlignment = Alignment.Center) {
        if (preview != null && !fit) {
            Image(
                BitmapPainter(preview, filterQuality = FilterQuality.Low),
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        }
        val loader = LocalImageLoader.current
        when {
            url != null && loader != null && !failed -> key(attempt) {
                AsyncImage(
                    model = url,
                    imageLoader = loader,
                    contentDescription = contentDescription,
                    contentScale = if (fit) ContentScale.Fit else ContentScale.Crop,
                    onError = {
                        if (fit && attempt == 0) retrying = true else failed = true
                    },
                    modifier = Modifier.fillMaxSize(),
                )
            }
            unsupported || failed -> Icon(SquirrelIcon.ImageOff, contentDescription = contentDescription, tint = scheme.inkMuted)
            preview == null -> Icon(SquirrelIcon.Image, contentDescription = contentDescription, tint = scheme.inkMuted)
        }
    }
}

/** A photo the blur filter hides: its blurhash under `--photo-shade`, `eye-off` in the middle. */
@Composable
private fun BlurredThumbnail(blurhash: String?, contentDescription: String?, modifier: Modifier) {
    val scheme = SquirrelTheme.scheme
    val decoded by produceState<ImageBitmap?>(null, blurhash) { value = blurhash?.let { blurhashBitmap(it) } }
    Box(modifier.background(scheme.tint), contentAlignment = Alignment.Center) {
        decoded?.let {
            Image(
                BitmapPainter(it, filterQuality = FilterQuality.Low),
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        }
        Box(Modifier.fillMaxSize().background(scheme.photoShade))
        Icon(
            SquirrelIcon.EyeOff,
            contentDescription = contentDescription?.let { "$it, blurred" },
            tint = scheme.photoRing,
        )
    }
}
