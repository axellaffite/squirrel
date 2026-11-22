package app.squirrel.android.sync

import android.content.Context
import app.squirrel.android.media.MediaAccessReader
import app.squirrel.android.media.MediaStoreSource
import app.squirrel.android.media.choice
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.AssetSource
import app.squirrel.shared.sync.Conditions
import app.squirrel.ui.backup.AlbumChoice
import app.squirrel.ui.backup.AlbumKind
import app.squirrel.ui.backup.BackupPlatform

/** Android's backup pieces: MediaStore, network and power, WorkManager. */
class AndroidBackup(context: Context) : BackupPlatform {
    val access = MediaAccessReader(context)
    private val mediaStore = MediaStoreSource(context, access)
    val scheduler = BackupScheduler(context)
    override val source: AssetSource = mediaStore
    override val conditions: Conditions = AndroidConditions(context)

    override fun locationsIncluded(): Boolean = access.locationsIncluded()
    override val albumKind = AlbumKind.Folders
    override suspend fun albums(): List<AlbumChoice> = mediaStore.albums().map { it.choice() }
    override fun schedule(settings: BackupSettings) = scheduler.apply(settings)
    override fun backUpNow(settings: BackupSettings) = scheduler.backUpNow(settings)
    override fun runSoon(settings: BackupSettings) = scheduler.onNewMedia(settings)
    override fun cancel() = scheduler.cancel()
}
