package app.squirrel.android

import android.Manifest
import android.content.ContentValues
import android.graphics.Bitmap
import android.graphics.Color
import android.media.ExifInterface
import android.net.Uri
import android.provider.MediaStore
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.cash.sqldelight.driver.android.AndroidSqliteDriver
import app.squirrel.android.media.MediaAccessReader
import app.squirrel.android.media.MediaPermissions
import app.squirrel.android.media.MediaStoreSource
import app.squirrel.android.sync.AndroidConditions
import app.squirrel.api.models.DevicePlatform
import app.squirrel.api.models.NodeKind
import app.squirrel.api.models.SyncCheckItem
import app.squirrel.api.models.SyncCheckRequest
import app.squirrel.shared.api.DeviceDetails
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.auth.KeystoreTokenStore
import app.squirrel.shared.auth.Session
import app.squirrel.shared.db.SquirrelDatabase
import app.squirrel.shared.db.createDatabase
import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.settings.SettingsStore
import app.squirrel.shared.sync.KtorChunkTransport
import app.squirrel.shared.sync.Queue
import app.squirrel.shared.sync.Scanner
import app.squirrel.shared.sync.StopCause
import app.squirrel.shared.sync.SyncEngine
import io.ktor.client.engine.okhttp.OkHttp
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Backs up photos from the emulator to a real server (mobile/scripts/mobile-e2e-server.sh).
 * Skipped unless the instrumentation argument `squirrelServer` is set, e.g.
 * `-Pandroid.testInstrumentationRunnerArguments.squirrelServer=http://localhost:8188`
 * (with `adb reverse tcp:8188 tcp:8188`).
 */
@RunWith(AndroidJUnit4::class)
class BackupEndToEndTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private val resolver = context.contentResolver
    private val args = InstrumentationRegistry.getArguments()
    private val server: String? = args.getString("squirrelServer")
    private val bucket = "SquirrelE2E${System.currentTimeMillis()}"
    private val inserted = mutableListOf<Uri>()
    private val images = MediaStore.Images.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
    private val videos = MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)

    @Before
    fun grant() {
        MediaPermissions.toRequest(android.os.Build.VERSION.SDK_INT)
            .forEach { instrumentation.uiAutomation.grantRuntimePermission(context.packageName, it) }
    }

    @After
    fun cleanUp() {
        inserted.forEach { runCatching { resolver.delete(it, null, null) } }
    }

    private fun jpeg(color: Int, exif: Boolean): ByteArray {
        val file = File.createTempFile("e2e", ".jpg", context.cacheDir)
        Bitmap.createBitmap(64, 48, Bitmap.Config.ARGB_8888).apply { eraseColor(color) }
            .compress(Bitmap.CompressFormat.JPEG, 90, file.outputStream())
        if (exif) {
            ExifInterface(file.path).apply {
                setAttribute(ExifInterface.TAG_DATETIME_ORIGINAL, "2023:05:20 14:03:00")
                setAttribute("OffsetTimeOriginal", "+02:00") // without it MediaStore leaves DATE_TAKEN empty
                setAttribute(ExifInterface.TAG_IMAGE_DESCRIPTION, bucket) // unique bytes per run
                setAttribute(ExifInterface.TAG_GPS_LATITUDE, "48/1,51/1,24/1")
                setAttribute(ExifInterface.TAG_GPS_LATITUDE_REF, "N")
                setAttribute(ExifInterface.TAG_GPS_LONGITUDE, "2/1,21/1,7/1")
                setAttribute(ExifInterface.TAG_GPS_LONGITUDE_REF, "E")
                saveAttributes()
            }
        }
        // Unique per run (after the JPEG's end marker), so an earlier run on the same server does
        // not make these "already backed up".
        return (file.readBytes() + bucket.encodeToByteArray()).also { file.delete() }
    }

    private fun insert(collection: Uri, name: String, mime: String, bytes: ByteArray, takenAt: Long? = null) {
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, name)
            put(MediaStore.MediaColumns.MIME_TYPE, mime)
            put(MediaStore.MediaColumns.RELATIVE_PATH, (if (mime.startsWith("video")) "Movies/" else "Pictures/") + bucket)
            put(MediaStore.MediaColumns.IS_PENDING, 1)
            takenAt?.let { put(MediaStore.MediaColumns.DATE_TAKEN, it) }
        }
        val uri = checkNotNull(resolver.insert(collection, values))
        inserted += uri
        resolver.openOutputStream(uri)!!.use { it.write(bytes) }
        resolver.update(uri, ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }, null, null)
    }

    @Test
    fun theKeystoreTokenStoreRoundTrips() = runBlocking {
        val db = createDatabase(AndroidSqliteDriver(SquirrelDatabase.Schema, context, null))
        val store = KeystoreTokenStore(db)
        store.save("sqr_round_trip")
        assertEquals("sqr_round_trip", store.load())
        assertTrue(db.secretQueries.get("refresh_token").executeAsOne().ciphertext.decodeToString() != "sqr_round_trip")
        store.clear()
        assertEquals(null, store.load())
    }

    @Test
    fun backsUpPhotosAndAVideoToTheServer() = runBlocking {
        val base = server
        assumeTrue("squirrelServer not set", base != null)
        val withExif = jpeg(Color.RED, exif = true)
        insert(images, "e2e-exif.jpg", "image/jpeg", withExif)
        // No EXIF date: the phone's time for it (here the file's) must reach the server as taken_at.
        insert(images, "e2e-no-exif.jpg", "image/jpeg", jpeg(Color.BLUE, exif = false))
        insert(images, "e2e-duplicate.jpg", "image/jpeg", withExif)
        insert(videos, "e2e-clip.mp4", "video/mp4", bucket.encodeToByteArray() + ByteArray(64 * 1024) { (it % 251).toByte() })

        val db = createDatabase(AndroidSqliteDriver(SquirrelDatabase.Schema, context, null))
        val source = MediaStoreSource(context, MediaAccessReader(context))
        val bucketId = source.albums().filter { it.name == bucket }.map { it.id }.toSet() // Pictures/ and Movies/
        val settings = SettingsStore(db).apply {
            setBackup(BackupSettings(wifiOnly = false, albums = AlbumFilter.Selected(bucketId)))
        }
        val session = Session(KeystoreTokenStore(db))
        val api = SquirrelApi(checkNotNull(base), OkHttp.create(), session)
        val signedIn = api.signIn(
            args.getString("squirrelUser") ?: "phone",
            args.getString("squirrelPassword") ?: "phone-e2e-password",
            DeviceDetails("E2E $bucket", DevicePlatform.android, android.os.Build.MODEL, "e2e"),
        )
        val queue = Queue(db)
        val engine = SyncEngine(
            source = source,
            api = api,
            queue = queue,
            scanner = Scanner(source, db, queue, settings, now = System::currentTimeMillis),
            transport = KtorChunkTransport(api.chunks),
            conditions = AndroidConditions(context),
            settings = settings,
            now = System::currentTimeMillis,
        )

        val summary = engine.runOnce()

        assertEquals(StopCause.Finished, summary.stoppedBy, engine.state.value.toString())
        assertEquals(3, summary.uploaded)
        assertEquals(1, summary.deduplicated)
        val root = checkNotNull(api.devices.getCurrentDevice().bodyOrThrow().backupFolderId)
        val years = api.files.listNodes(parentId = root).bodyOrThrow().items.filter { it.kind == NodeKind.folder }
        val names = years.flatMap { year -> api.files.listNodes(parentId = year.id).bodyOrThrow().items.map { "${year.name}/${it.name}" } }
        assertEquals(
            setOf("e2e-exif.jpg", "e2e-duplicate.jpg", "e2e-no-exif.jpg", "e2e-clip.mp4"),
            names.map { it.substringAfter('/') }.toSet(),
        )
        // Year folders come from the phone's capture time: EXIF 2023 for the tagged photo and its
        // copy, the file time (this year) for the untagged photo and the clip.
        val thisYear = java.time.Year.now().value
        assertTrue("2023/e2e-exif.jpg" in names && "2023/e2e-duplicate.jpg" in names, names.toString())
        assertTrue("$thisYear/e2e-no-exif.jpg" in names && "$thisYear/e2e-clip.mp4" in names, names.toString())
        val done = db.localAssetQueries.counts().executeAsList().associate { it.state to it.n }
        assertEquals(mapOf("done" to 4L), done)
        // Nothing left to send: the server knows every asset.
        val all = source.changes(null, app.squirrel.shared.sync.SourceFilter(true, AlbumFilter.Selected(bucketId)))
            .assets.filter { it.album in bucketId }
        val hashes = all.map { asset -> SyncCheckItem(asset.deviceAssetId, checkNotNull(queue.get(asset.deviceAssetId)?.sha256)) }
        assertEquals(emptyList(), api.sync.syncCheck(SyncCheckRequest(hashes)).bodyOrThrow().missing)
        println("E2E device=${signedIn.device.name} uploaded=${names.sorted()}")
    }
}
