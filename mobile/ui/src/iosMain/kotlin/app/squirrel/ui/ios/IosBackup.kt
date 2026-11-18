package app.squirrel.ui.ios

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.db.SquirrelDatabase
import app.squirrel.shared.ios.background.BackgroundRunner
import app.squirrel.shared.ios.background.BackgroundTasks
import app.squirrel.shared.ios.background.IosConditions
import app.squirrel.shared.ios.photos.AssetExporter
import app.squirrel.shared.ios.photos.PhotoKitSource
import app.squirrel.shared.ios.transfer.BackgroundTransport
import app.squirrel.shared.ios.transfer.TransferSession
import app.squirrel.shared.ios.transfer.TransportStore
import app.squirrel.shared.ios.transfer.backgroundConfiguration
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.settings.SettingsStore
import app.squirrel.shared.sync.AssetSource
import app.squirrel.shared.sync.ChunkTransport
import app.squirrel.shared.sync.Conditions
import app.squirrel.shared.sync.Queue
import app.squirrel.ui.backup.AlbumChoice
import app.squirrel.ui.backup.BackupPlatform
import kotlinx.coroutines.CoroutineScope

/** iOS backup: PhotoKit, the background URLSession, BGTaskScheduler. */
class IosBackup(
    private val database: () -> SquirrelDatabase,
    private val settings: () -> SettingsStore,
    runner: BackgroundRunner,
    scope: CoroutineScope,
    private val now: () -> Long,
) : BackupPlatform {
    val iosConditions = IosConditions()
    private val photoKit = PhotoKitSource(
        AssetExporter(),
        allowNetwork = { !settings().backup().wifiOnly || iosConditions.unmetered() },
        uploading = { Queue(database()).uploadingIds() },
    )
    val tasks = BackgroundTasks(runner, scope, now)

    /** One background session per process (the system keys it by identifier). */
    val transfers: TransferSession by lazy {
        TransportStore(database(), now).prune()
        TransferSession(backgroundConfiguration(), TransportStore(database(), now), "${cachesPath()}/chunks", ::backgroundEventsFinished)
    }

    override val source: AssetSource = photoKit
    override val conditions: Conditions = iosConditions

    override fun locationsIncluded(): Boolean = true // PhotoKit originals keep their metadata

    override suspend fun albums(): List<AlbumChoice> = photoKit.albums().map { AlbumChoice(it.id, it.name, it.count) }

    override fun schedule(settings: BackupSettings) = tasks.schedule(settings)

    override fun runSoon(settings: BackupSettings) = tasks.runSoon()

    override fun backUpNow(settings: BackupSettings) = tasks.backUpNow()

    override fun cancel() {
        tasks.cancel()
        transfers.cancelAll()
    }

    override fun transport(api: SquirrelApi): ChunkTransport =
        BackgroundTransport(api.baseUrl, transfers) { settings().backup().wifiOnly }
}
