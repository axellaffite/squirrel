package app.squirrel.ui.images

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
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
import kotlinx.coroutines.withContext
import coil3.compose.AsyncImage
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.components.Icon

/** The signed-in device's image loader (null while signed out). */
val LocalImageLoader = staticCompositionLocalOf<ImageLoader?> { null }

private const val HASH_W = 16
private const val HASH_H = 12
private const val HASH_CACHE = 256

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
) {
    val scheme = SquirrelTheme.scheme
    val decoded by produceState<ImageBitmap?>(null, blurhash) { value = blurhash?.let { blurhashBitmap(it) } }
    // A load that failed (offline, deleted, refused) shows `image-off`, not a blurhash forever.
    var failed by remember(url) { mutableStateOf(false) }
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
            url != null && loader != null && !failed -> AsyncImage(
                model = url,
                imageLoader = loader,
                contentDescription = contentDescription,
                contentScale = if (fit) ContentScale.Fit else ContentScale.Crop,
                onError = { failed = true },
                modifier = Modifier.fillMaxSize(),
            )
            unsupported || failed -> Icon(SquirrelIcon.ImageOff, contentDescription = contentDescription, tint = scheme.inkMuted)
            preview == null -> Icon(SquirrelIcon.Image, contentDescription = contentDescription, tint = scheme.inkMuted)
        }
    }
}
