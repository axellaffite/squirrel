package app.squirrel.ui.photos

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.testing.FakePhotoSaver
import app.squirrel.ui.testing.apiError
import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import app.squirrel.ui.testing.text
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Download from the viewer and the selection bar: one photo to the library, several as one zip. */
class PhotoDownloadsTest {
    private val sent = mutableListOf<String>()

    private fun details(id: String, size: Long, mime: String = "image/jpeg") =
        """{"asset":{"id":"$id","kind":"photo","name":"$id.jpg","taken_at":"2024-07-14T18:30:05","thumbnail":{"state":"ready","version":1},""" +
            """"width":4,"height":3,"mime":"$mime"},"size":$size,"albums":[],"tags":[],"people":[]}"""

    private fun ticket(size: Long, count: Int = 3) =
        """{"ticket":"tk1","name":"Squirrel $count photos 2026-11-26.zip","size":$size,"count":$count}"""

    private suspend fun api(photoSize: Long = 5_000_000, archive: (() -> Pair<HttpStatusCode, String>)? = null): SquirrelApi = testApi { request ->
        val path = request.url.encodedPath
        when {
            path == "/api/files/archive" && request.method == HttpMethod.Post -> {
                sent += request.text()
                val (status, body) = archive?.invoke() ?: (HttpStatusCode.Created to ticket(40_000_000))
                if (status.value >= 400) apiError(status, "archive_too_large", "too much to put in one zip: select fewer files") else json(body, status)
            }
            path.startsWith("/api/photos/") -> json(details(path.substringAfterLast('/'), photoSize))
            else -> respondError(HttpStatusCode.NotFound)
        }
    }

    private fun withScope(block: suspend (CoroutineScope) -> Unit) = runReal {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        try {
            block(scope)
        } finally {
            scope.cancel()
        }
    }

    @Test
    fun onePhotoIsSavedToTheLibraryAtOnce() = withScope { scope ->
        val fake = FakePhotoSaver()
        val downloads = PhotoDownloads(api(), fake.saver, scope)

        downloads.download(listOf("p1"))
        val done = downloads.state.await { it.message != null }

        assertEquals(listOf(FakePhotoSaver.Original("p1", "p1.jpg", "image/jpeg")), fake.originals)
        assertEquals("Saved to Pictures.", done.message)
        assertFalse(done.failed)
        assertNull(done.progress)
        assertTrue(fake.archives.isEmpty())
        assertTrue(sent.isEmpty())
    }

    @Test
    fun progressShowsWhileTheOriginalDownloads() = withScope { scope ->
        val fake = FakePhotoSaver(progress = 0.25f).apply { gate = CompletableDeferred() }
        val downloads = PhotoDownloads(api(), fake.saver, scope)

        downloads.download(listOf("p1"))
        val running = downloads.state.await { it.progress == 0.25f }
        assertEquals("Downloading the original…", running.progressLabel)

        downloads.download(listOf("p2")) // one at a time
        fake.gate?.complete(Unit)
        downloads.state.await { it.progress == null && it.message != null }
        assertEquals(listOf("p1"), fake.originals.map { it.id })
    }

    @Test
    fun aLargeOriginalAsksFirst() = withScope { scope ->
        val fake = FakePhotoSaver()
        val downloads = PhotoDownloads(api(photoSize = 1_400_000_000), fake.saver, scope)

        downloads.download(listOf("v1"))
        val asking = downloads.state.await { it.confirm != null }
        assertEquals("Download 1.4 GB?", asking.confirmText)
        assertNull(asking.progress)
        assertTrue(fake.originals.isEmpty())

        downloads.confirm()
        downloads.state.await { it.message != null }
        assertEquals(listOf("v1"), fake.originals.map { it.id })
        assertNull(downloads.state.value.confirm)
    }

    @Test
    fun cancellingTheQuestionSavesNothing() = withScope { scope ->
        val fake = FakePhotoSaver()
        val downloads = PhotoDownloads(api(photoSize = 1_400_000_000), fake.saver, scope)

        downloads.download(listOf("v1"))
        downloads.state.await { it.confirm != null }
        downloads.dismiss()

        assertNull(downloads.state.value.confirm)
        assertNull(downloads.state.value.progress)
        assertTrue(fake.originals.isEmpty())
    }

    @Test
    fun severalPhotosGoIntoOneZip() = withScope { scope ->
        val fake = FakePhotoSaver()
        val downloads = PhotoDownloads(api(), fake.saver, scope)

        downloads.download(listOf("p1", "p2", "p3"))
        val running = downloads.state.await { it.progress != null || it.message != null }
        if (running.progress != null) assertEquals("Downloading 3 photos…", running.progressLabel)
        val done = downloads.state.await { it.message != null }

        assertEquals("""{"node_ids":["p1","p2","p3"]}""", sent.single())
        assertEquals(
            listOf(FakePhotoSaver.Archive("https://home.example/api/files/archive/tk1", "Squirrel 3 photos 2026-11-26.zip")),
            fake.archives,
        )
        assertEquals("Saved to Downloads.", done.message)
        assertTrue(fake.originals.isEmpty())
    }

    @Test
    fun aLargeZipAsksFirst() = withScope { scope ->
        val fake = FakePhotoSaver()
        val downloads = PhotoDownloads(api(archive = { HttpStatusCode.Created to ticket(2_500_000_000, count = 12) }), fake.saver, scope)

        downloads.download(listOf("p1", "p2"))
        val asking = downloads.state.await { it.confirm != null }
        assertEquals("Download 2.5 GB?", asking.confirmText)
        assertEquals("12 photos make one large zip.", asking.confirmDetail)
        assertTrue(fake.archives.isEmpty())

        downloads.confirm()
        downloads.state.await { it.message != null }
        assertEquals(1, fake.archives.size)
    }

    @Test
    fun tooMuchForOneZipSaysToSelectFewer() = withScope { scope ->
        val fake = FakePhotoSaver()
        val downloads = PhotoDownloads(api(archive = { HttpStatusCode.PayloadTooLarge to "" }), fake.saver, scope)

        downloads.download(listOf("p1", "p2"))
        val done = downloads.state.await { it.message != null }

        assertEquals("Too much to put in one zip: select fewer photos.", done.message)
        assertTrue(done.failed)
        assertNull(done.progress)
        assertTrue(fake.archives.isEmpty())
    }

    @Test
    fun aFailedSaveShowsWhy() = withScope { scope ->
        val fake = FakePhotoSaver(original = SaveResult.Failed("Photos could not keep p1.jpg."))
        val downloads = PhotoDownloads(api(), fake.saver, scope)

        downloads.download(listOf("p1"))
        val done = downloads.state.await { it.message != null }

        assertEquals("Photos could not keep p1.jpg.", done.message)
        assertTrue(done.failed)
    }

    @Test
    fun aZipHandedToTheShareSheetSaysNothing() = withScope { scope ->
        val fake = FakePhotoSaver(archive = SaveResult.Handed)
        val downloads = PhotoDownloads(api(), fake.saver, scope)

        downloads.download(listOf("p1", "p2"))
        downloads.state.await { it.progress == null && fake.archives.isNotEmpty() }

        assertNull(downloads.state.value.message)
        assertFalse(downloads.state.value.failed)
    }

    @Test
    fun anUnreachableServerSaysSo() = withScope { scope ->
        val fake = FakePhotoSaver()
        val downloads = PhotoDownloads(testApi { respondError(HttpStatusCode.BadGateway) }, fake.saver, scope)

        downloads.download(listOf("p1"))
        val done = downloads.state.await { it.message != null }

        assertEquals(UNREACHABLE, done.message)
        assertTrue(done.failed)
        assertTrue(fake.originals.isEmpty())
    }

    @Test
    fun aNewDownloadClearsTheLastMessage() = withScope { scope ->
        val fake = FakePhotoSaver()
        val downloads = PhotoDownloads(api(), fake.saver, scope)
        downloads.download(listOf("p1"))
        downloads.state.await { it.message != null }

        fake.gate = CompletableDeferred()
        downloads.download(listOf("p2"))
        assertNull(downloads.state.value.message)
        fake.gate?.complete(Unit)
        downloads.state.await { it.message != null }
    }

    @Test
    fun nothingSelectedDoesNothing() = withScope { scope ->
        val fake = FakePhotoSaver()
        val downloads = PhotoDownloads(api(), fake.saver, scope)
        downloads.download(emptyList())
        assertEquals(DownloadState(), downloads.state.value)
    }

    @Test
    fun theButtonSaysWhatItDownloads() {
        assertEquals("Download 3 photos", downloadLabel(3, null))
        assertEquals("Download IMG_1.jpg", downloadLabel(1, "IMG_1.jpg"))
        assertEquals("Download 1 photo", downloadLabel(1, null))
    }
}
