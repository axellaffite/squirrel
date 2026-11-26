package app.squirrel.ui.images

import coil3.PlatformContext
import coil3.network.CacheStrategy
import coil3.network.NetworkRequest
import coil3.network.NetworkResponse
import coil3.request.Options
import coil3.size.Size
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.assertNull

class SuccessOnlyCacheTest {
    private val request = NetworkRequest("https://squirrel.test/api/files/x/thumbnail?size=preview&v=1")
    private val options = Options(PlatformContext.INSTANCE, Size.ORIGINAL)

    @Test
    fun `an error response is never written to the disk cache`() = runTest {
        val result = SuccessOnlyCache.write(null, request, NetworkResponse(code = 404), options)
        assertEquals(CacheStrategy.WriteResult.DISABLED, result)
    }

    @Test
    fun `a successful response is written`() = runTest {
        val result = SuccessOnlyCache.write(null, request, NetworkResponse(code = 200), options)
        assertNotEquals(CacheStrategy.WriteResult.DISABLED, result)
    }

    @Test
    fun `a cached error is ignored and the image is fetched again`() = runTest {
        val result = SuccessOnlyCache.read(NetworkResponse(code = 404), request, options)
        assertNull(result.response)
        assertEquals(request, result.request)
    }
}
