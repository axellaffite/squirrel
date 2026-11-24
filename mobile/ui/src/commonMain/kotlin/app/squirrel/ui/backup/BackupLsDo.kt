package app.squirrel.ui.backup

import app.squirrel.lsdo.Doc
import app.squirrel.lsdo.LsDoScreen
import app.squirrel.lsdo.lsdoSpec
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.shared.sync.QueuedAsset
import app.squirrel.shared.sync.SyncState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** What the backup is doing, as an agent names it. */
@Serializable
enum class BackupStep {
    @SerialName("idle") Idle,
    @SerialName("scanning") Scanning,
    @SerialName("hashing") Hashing,
    @SerialName("checking") Checking,
    @SerialName("uploading") Uploading,
    @SerialName("paused") Paused,
    @SerialName("waiting_for_network") WaitingForNetwork,
    @SerialName("error") Error,
}

/** How much of the photo library Squirrel may read. */
@Serializable
enum class PhotoAccess {
    @SerialName("full") Full,
    @SerialName("partial") Partial,
    @SerialName("no_videos") NoVideos,
    @SerialName("none") None,
}

@Serializable
data class FailedAsset(val name: String, val error: String)

/** The Backup screen as an agent reads it: the figures as numbers, the texts as shown. */
@Serializable
data class BackupView(
    @Doc("The status is still being read") val loading: Boolean = true,
    val step: BackupStep = BackupStep.Idle,
    @Doc("A run is going on: \"Back up now\" is disabled") val running: Boolean = false,
    @Doc("The status card's figure: \"1 204 left\", \"Everything is backed up\"") val headline: String = "",
    @Doc("Size left and the network rule") val detail: String = "",
    val lastRun: String = "",
    @Doc("0–1 while uploading or preparing; null otherwise") val progress: Float? = null,
    val progressLabel: String? = null,
    @Doc("Photos and videos selected for backup") val total: Long = 0,
    val backedUp: Long = 0,
    val waiting: Long = 0,
    val failed: Long = 0,
    val bytesLeft: Long = 0,
    @Doc("Why the backup is paused or stopped") val notice: String? = null,
    val noticeIsError: Boolean = false,
    val access: PhotoAccess = PhotoAccess.Full,
    @Doc("What partial or video-less access means for the backup") val accessNote: String? = null,
    val locationWarning: String? = null,
    val batteryNotice: String? = null,
    @Doc("The \"Not backed up\" sheet's rows while it is open; null when closed") val failures: List<FailedAsset>? = null,
)

internal fun backupView(snapshot: BackupSnapshot?, failures: List<QueuedAsset>?): BackupView {
    snapshot ?: return BackupView()
    val status = snapshot.status
    val ui = snapshot.ui
    return BackupView(
        loading = false,
        step = stepOf(status.state),
        running = ui.running,
        headline = ui.featured,
        detail = ui.detail,
        lastRun = ui.lastRun,
        progress = ui.progress,
        progressLabel = ui.progressLabel,
        total = status.total,
        backedUp = status.backedUp,
        waiting = status.left,
        failed = status.failed,
        bytesLeft = status.bytesLeft,
        notice = ui.notice,
        noticeIsError = ui.noticeIsError,
        access = when (status.access) {
            MediaAccess.Full -> PhotoAccess.Full
            is MediaAccess.Partial -> PhotoAccess.Partial
            MediaAccess.NoVideos -> PhotoAccess.NoVideos
            MediaAccess.None -> PhotoAccess.None
        },
        accessNote = when (val access = ui.access) {
            is AccessUi.Partial -> access.text
            is AccessUi.NoVideos -> access.text
            AccessUi.Full, AccessUi.None -> null
        },
        locationWarning = ui.locationWarning,
        batteryNotice = ui.batteryNotice,
        failures = failures?.map { FailedAsset(it.name, it.lastError ?: "Unknown error") },
    )
}

private fun stepOf(state: SyncState): BackupStep = when (state) {
    SyncState.Idle -> BackupStep.Idle
    SyncState.Scanning -> BackupStep.Scanning
    is SyncState.Hashing -> BackupStep.Hashing
    SyncState.Checking -> BackupStep.Checking
    is SyncState.Uploading -> BackupStep.Uploading
    is SyncState.Paused -> BackupStep.Paused
    SyncState.WaitingForNetwork -> BackupStep.WaitingForNetwork
    is SyncState.Error -> BackupStep.Error
}

/**
 * What an agent can do on the Backup screen (LS-DO). Granting photo access and the battery
 * setting open system dialogs a person answers: they are not here.
 */
@Serializable
sealed interface BackupIntent {
    @Serializable @SerialName("back_up_now") @Doc("Start a run now, as the \"Back up now\" button does. Not while one is running, nor without photo access")
    data object BackUpNow : BackupIntent

    @Serializable @SerialName("refresh") @Doc("Read photo access and the battery setting again, as coming back to the app does")
    data object Refresh : BackupIntent

    @Serializable @SerialName("show_failures") @Doc("Open the \"Not backed up\" sheet (failures in state). Only when something failed")
    data object ShowFailures : BackupIntent

    @Serializable @SerialName("retry_failures") @Doc("\"Try again\" on the \"Not backed up\" sheet: closes it and starts a run. Only while it is open")
    data object RetryFailures : BackupIntent

    @Serializable @SerialName("hide_failures") @Doc("Close the \"Not backed up\" sheet. Only while it is open")
    data object HideFailures : BackupIntent

    @Serializable @SerialName("open_settings") @Doc("Open Backup settings (the backup_settings screen). Not without photo access")
    data object OpenSettings : BackupIntent
}

/**
 * The Backup screen as an LS-DO screen ("backup"). A wrapper, not the model itself: the model's
 * state is not serializable, and opening settings is the tab's own saveable Compose state,
 * set through [openSettings] exactly as the settings row does.
 */
class BackupLsDo(private val vm: BackupViewModel, private val openSettings: () -> Unit) : LsDoScreen<BackupView, BackupIntent> {
    override val lsdo = lsdoSpec<BackupView, BackupIntent>()

    override val state: StateFlow<BackupView> get() = vm.view

    override suspend fun perform(intent: BackupIntent): String? {
        when (intent) {
            BackupIntent.BackUpNow -> vm.backUpNow()
            BackupIntent.Refresh -> vm.onResume()
            BackupIntent.ShowFailures -> vm.showFailures()
            BackupIntent.RetryFailures -> vm.retryFailures()
            BackupIntent.HideFailures -> vm.hideFailures()
            BackupIntent.OpenSettings -> openSettings()
        }
        return null
    }

    /** Without photo access the screen only asks for it; the failure sheet has its own buttons. */
    override fun availableIntents(state: BackupView): Set<String> = buildSet {
        add("refresh")
        if (state.loading || state.access == PhotoAccess.None) return@buildSet
        add("open_settings")
        if (!state.running) add("back_up_now")
        if (state.failures != null) addAll(listOf("retry_failures", "hide_failures"))
        else if (state.failed > 0) add("show_failures")
    }

    override fun isBusy(state: BackupView): Boolean = state.loading
}
