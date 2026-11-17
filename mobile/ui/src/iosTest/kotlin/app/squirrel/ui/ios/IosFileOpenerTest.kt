package app.squirrel.ui.ios

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.auth.InMemoryTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.auth.Tokens
import app.squirrel.ui.files.HandoffResult
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.coroutines.test.runTest
import platform.Foundation.NSFileManager
import platform.Foundation.NSTemporaryDirectory
import platform.Foundation.NSUUID
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

@OptIn(ExperimentalForeignApi::class)
class IosFileOpenerTest {
    private val dir = "${NSTemporaryDirectory()}open-test-${NSUUID().UUIDString}"
    private val files = NSFileManager.defaultManager

    @AfterTest
    fun cleanUp() {
        files.removeItemAtPath(dir, null)
    }

    private suspend fun api(status: HttpStatusCode, body: ByteArray): SquirrelApi {
        val session = Session(InMemoryTokenStore()).apply { signedIn(Tokens("sqa", "sqr")) }
        val engine = MockEngine {
            respond(body, status, headersOf(HttpHeaders.ContentLength, body.size.toString()))
        }
        return SquirrelApi("https://home.example", engine, session)
    }

    @Test
    fun theOriginalIsDownloadedUnderASafeName() = runTest {
        val bytes = ByteArray(200_000) { (it % 251).toByte() }
        val progress = mutableListOf<Float>()
        val result = IosFileOpener(dir) { null }.open(api(HttpStatusCode.OK, bytes), "n1", "../notes.txt", "text/plain", share = false) {
            progress += it
        }
        // No window to present from.
        assertEquals(HandoffResult.NoApp, result)
        val saved = "$dir/n1/_notes.txt"
        assertTrue(files.fileExistsAtPath(saved))
        assertEquals(bytes.size.toLong(), (checkNotNull(files.attributesOfItemAtPath(saved, null))["NSFileSize"] as Number).toLong())
        assertFalse(files.fileExistsAtPath("$saved.part"))
        assertEquals(1f, progress.last())
    }

    @Test
    fun aRefusedDownloadLeavesNothingBehind() = runTest {
        val result = IosFileOpener(dir) { null }.open(api(HttpStatusCode.NotFound, ByteArray(0)), "n2", "gone.pdf", null, share = true) {}
        assertIs<HandoffResult.Failed>(result)
        assertFalse(files.fileExistsAtPath("$dir/n2/gone.pdf"))
        assertFalse(files.fileExistsAtPath("$dir/n2/gone.pdf.part"))
    }

    @Test
    fun openingAnotherFileClearsThePreviousOne() = runTest {
        val opener = IosFileOpener(dir) { null }
        opener.open(api(HttpStatusCode.OK, ByteArray(10)), "a", "a.txt", null, share = false) {}
        opener.open(api(HttpStatusCode.OK, ByteArray(10)), "b", "b.txt", null, share = false) {}
        assertFalse(files.fileExistsAtPath("$dir/a/a.txt"))
        assertTrue(files.fileExistsAtPath("$dir/b/b.txt"))
    }
}
