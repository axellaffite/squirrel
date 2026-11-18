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
import platform.Foundation.NSFileProtectionCompleteUntilFirstUserAuthentication
import platform.Foundation.NSFileProtectionKey
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
    fun openingAnotherFileKeepsTheOneASheetMayStillRead() = runTest {
        val opener = IosFileOpener(dir) { null }
        opener.open(api(HttpStatusCode.OK, ByteArray(10)), "a", "a.txt", null, share = false) {}
        opener.open(api(HttpStatusCode.OK, ByteArray(10)), "b", "b.txt", null, share = false) {}
        assertTrue(files.fileExistsAtPath("$dir/a/a.txt"))
        assertTrue(files.fileExistsAtPath("$dir/b/b.txt"))
        opener.clear() // at launch and at sign-out
        assertFalse(files.fileExistsAtPath("$dir/a/a.txt"))
        assertFalse(files.fileExistsAtPath("$dir/b/b.txt"))
    }

    @Test
    fun theSameFileOpenedTwiceIsReplaced() = runTest {
        val opener = IosFileOpener(dir) { null }
        opener.open(api(HttpStatusCode.OK, ByteArray(10)), "a", "a.txt", null, share = false) {}
        assertEquals(HandoffResult.NoApp, opener.open(api(HttpStatusCode.OK, ByteArray(20)), "a", "a.txt", null, share = false) {})
        assertEquals(20L, (checkNotNull(files.attributesOfItemAtPath("$dir/a/a.txt", null))["NSFileSize"] as Number).toLong())
    }

    @Test
    fun downloadsAreProtectedUntilTheFirstUnlock() = runTest {
        IosFileOpener(dir) { null }.open(api(HttpStatusCode.OK, ByteArray(10)), "p", "p.txt", null, share = false) {}
        val protection = files.attributesOfItemAtPath("$dir/p/p.txt", null)?.get(NSFileProtectionKey)
        // The simulator may not report data protection; where it does, it is the one asked for.
        if (protection != null) assertEquals<Any?>(NSFileProtectionCompleteUntilFirstUserAuthentication, protection)
    }
}
