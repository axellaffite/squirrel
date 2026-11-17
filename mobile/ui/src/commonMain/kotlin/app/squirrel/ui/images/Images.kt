package app.squirrel.ui.images

import coil3.ImageLoader
import coil3.PlatformContext
import coil3.disk.DiskCache
import coil3.memory.MemoryCache
import coil3.network.ktor3.KtorNetworkFetcherFactory
import io.ktor.client.HttpClient
import okio.Path

/** Where images live on this platform: its context and the app-private cache directory. */
interface ImagesPlatform {
    val context: PlatformContext

    /** Inside the app's private cache (Android `cacheDir`): never shared storage. */
    val cacheDir: Path

    /** Deletes every cached image file (thumbnails are private data). */
    fun clearFiles()
}

/** A share of the app's memory for decoded images; the rest of the UI keeps the remainder. */
private const val MEMORY_SHARE = 0.15
private const val DISK_BYTES = 256L * 1024 * 1024

/**
 * Coil over the authenticated Ktor client: thumbnails and previews come with the device's bearer
 * token and its refresh. Thumbnail URLs carry `v`, so cached copies never go stale.
 */
fun squirrelImageLoader(platform: ImagesPlatform, client: HttpClient): ImageLoader =
    ImageLoader.Builder(platform.context)
        .components { add(KtorNetworkFetcherFactory(httpClient = { client })) }
        .memoryCache { MemoryCache.Builder().maxSizePercent(platform.context, MEMORY_SHARE).build() }
        .diskCache { DiskCache.Builder().directory(platform.cacheDir).maxSizeBytes(DISK_BYTES).build() }
        .build()

/** Forgets every image, in memory and on disk (sign-out, revocation, another account). */
fun ImageLoader.forgetEverything() {
    memoryCache?.clear()
    diskCache?.clear()
    shutdown()
}
