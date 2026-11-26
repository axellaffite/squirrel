package app.squirrel.ui.images

import coil3.ImageLoader
import coil3.PlatformContext
import coil3.disk.DiskCache
import coil3.memory.MemoryCache
import coil3.network.CacheStrategy
import coil3.network.NetworkRequest
import coil3.network.NetworkResponse
import coil3.network.ktor3.KtorNetworkFetcherFactory
import coil3.request.Options
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
        .components { add(KtorNetworkFetcherFactory(httpClient = { client }, cacheStrategy = { SuccessOnlyCache })) }
        .memoryCache { MemoryCache.Builder().maxSizePercent(platform.context, MEMORY_SHARE).build() }
        .diskCache { DiskCache.Builder().directory(platform.cacheDir).maxSizeBytes(DISK_BYTES).build() }
        .build()

/** Forgets every image, in memory and on disk (sign-out, revocation, another account). */
/**
 * Coil's default strategy also stores error responses, so one 404 (a preview the server could not
 * render yet) would be served from disk forever for that URL. Only 2xx responses are written, and
 * an error already on disk is ignored: the image is fetched again.
 */
internal object SuccessOnlyCache : CacheStrategy {
    override suspend fun read(
        cacheResponse: NetworkResponse,
        networkRequest: NetworkRequest,
        options: Options,
    ): CacheStrategy.ReadResult =
        if (cacheResponse.isSuccess()) {
            CacheStrategy.DEFAULT.read(cacheResponse, networkRequest, options)
        } else {
            CacheStrategy.ReadResult(networkRequest)
        }

    override suspend fun write(
        cacheResponse: NetworkResponse?,
        networkRequest: NetworkRequest,
        networkResponse: NetworkResponse,
        options: Options,
    ): CacheStrategy.WriteResult =
        if (networkResponse.isSuccess()) {
            CacheStrategy.DEFAULT.write(cacheResponse, networkRequest, networkResponse, options)
        } else {
            CacheStrategy.WriteResult.DISABLED
        }

    private fun NetworkResponse.isSuccess(): Boolean = code in 200..299
}

fun ImageLoader.forgetEverything() {
    memoryCache?.clear()
    diskCache?.clear()
    shutdown()
}
