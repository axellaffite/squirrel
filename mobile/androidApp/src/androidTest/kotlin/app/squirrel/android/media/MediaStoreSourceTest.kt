package app.squirrel.android.media

import android.Manifest
import android.content.ContentValues
import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.provider.MediaStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.sync.SourceFilter
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

@RunWith(AndroidJUnit4::class)
class MediaStoreSourceTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val resolver = context.contentResolver
    private val source = MediaStoreSource(context, MediaAccessReader(context))
    private val inserted = mutableListOf<Uri>()
    private val bucket = "SquirrelTest${System.nanoTime()}"
    private val all = SourceFilter(includeVideos = true, albums = AlbumFilter.All)

    @Before
    fun grant() {
        MediaPermissions.toRequest(android.os.Build.VERSION.SDK_INT)
            .filter { it != Manifest.permission.POST_NOTIFICATIONS }
            .forEach { instrumentation.uiAutomation.grantRuntimePermission(context.packageName, it) }
    }

    @After
    fun cleanUp() {
        inserted.forEach { runCatching { resolver.delete(it, null, null) } } // some are already gone
    }

    private fun jpeg(color: Int): ByteArray {
        val bitmap = Bitmap.createBitmap(32, 24, Bitmap.Config.ARGB_8888).apply { eraseColor(color) }
        return ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.JPEG, 90, it) }.toByteArray()
    }

    /** Inserts through the IS_PENDING dance, as a camera app would. */
    private fun insert(collection: Uri, name: String, mime: String, bytes: ByteArray): Uri {
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, name)
            put(MediaStore.MediaColumns.MIME_TYPE, mime)
            put(MediaStore.MediaColumns.RELATIVE_PATH, (if (mime.startsWith("video")) "Movies/" else "Pictures/") + bucket)
            put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val uri = checkNotNull(resolver.insert(collection, values))
        inserted += uri
        resolver.openOutputStream(uri)!!.use { it.write(bytes) }
        resolver.update(uri, ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }, null, null)
        return uri
    }

    private val imagesUri = MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
    private val videoUri = MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)

    @Test
    fun scansReadsAndFollowsChanges() = runBlocking {
        val red = jpeg(Color.RED)
        insert(imagesUri, "a.jpg", "image/jpeg", red)
        insert(imagesUri, "b.jpg", "image/jpeg", jpeg(Color.BLUE))
        insert(videoUri, "c.mp4", "video/mp4", ByteArray(4096) { it.toByte() })

        val first = source.changes(null, all)
        val bucketId = source.albums().single { it.name == bucket }.id
        val ours = first.assets.filter { it.album == bucketId }.sortedBy { it.name }
        assertTrue(first.full)
        assertEquals(listOf("a.jpg", "b.jpg", "c.mp4"), ours.map { it.name })
        assertEquals(listOf("photo", "photo", "video"), ours.map { it.kind })
        assertEquals(red.size.toLong(), ours[0].size)
        assertTrue(ours[0].deviceAssetId.startsWith("images:") && ours[2].deviceAssetId.startsWith("video:"))

        insert(imagesUri, "d.jpg", "image/jpeg", jpeg(Color.GREEN))
        val next = source.changes(first.token, all)
        assertEquals(false, next.full)
        assertEquals(listOf("d.jpg"), next.assets.filter { it.album == bucketId }.map { it.name })

        val reader = assertNotNull(source.open(ours[0].deviceAssetId))
        val bytes = ByteArray(reader.size.toInt())
        var read = 0
        while (read < bytes.size) read += reader.read(read.toLong(), bytes, read, bytes.size - read)
        reader.close()
        assertContentEquals(red, bytes)
        assertEquals(ours[0].modifiedAt, reader.modifiedAt)

        resolver.delete(inserted.first(), null, null)
        assertNull(source.open(ours[0].deviceAssetId))
        assertTrue(source.albums().any { it.id == bucketId && it.name == bucket && it.count == 3 })
    }
}
