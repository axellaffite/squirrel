package app.squirrel.ui.backup

import app.squirrel.lsdo.Doc
import app.squirrel.lsdo.LsDoScreen
import app.squirrel.lsdo.lsdoSpec
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Backup › Settings as an agent reads it (the albums are on the album_picker screen). */
@Serializable
data class BackupSettingsView(
    @Doc("Back up in the background, when new photos appear and every few hours; off: only \"Back up now\"") val autoBackup: Boolean,
    val wifiOnly: Boolean,
    val chargingOnly: Boolean,
    val includeVideos: Boolean,
    @Doc("This phone's name as typed in the field") val deviceName: String,
    @Doc("The name the server kept") val savedName: String,
    @Doc("Why the name was not saved") val nameError: String?,
    @Doc("A new name is being saved") val renaming: Boolean,
    @Doc("Where this phone's backups go") val folderHint: String,
)

internal fun settingsView(state: BackupSettingsState) = BackupSettingsView(
    autoBackup = state.settings.autoBackup,
    wifiOnly = state.settings.wifiOnly,
    chargingOnly = state.settings.chargingOnly,
    includeVideos = state.settings.includeVideos,
    deviceName = state.deviceName,
    savedName = state.savedName,
    nameError = state.nameError,
    renaming = state.renaming,
    folderHint = state.folderHint,
)

/** What an agent can do in Backup › Settings (LS-DO); each maps to the [BackupSettingsViewModel] call the UI makes. */
@Serializable
sealed interface BackupSettingsIntent {
    @Serializable @SerialName("set_auto_backup") @Doc("The \"Back up automatically\" toggle")
    data class SetAutoBackup(val on: Boolean) : BackupSettingsIntent

    @Serializable @SerialName("set_wifi_only") @Doc("The \"Only on Wi-Fi\" toggle")
    data class SetWifiOnly(val on: Boolean) : BackupSettingsIntent

    @Serializable @SerialName("set_charging_only") @Doc("The \"Only while charging\" toggle")
    data class SetChargingOnly(val on: Boolean) : BackupSettingsIntent

    @Serializable @SerialName("set_include_videos") @Doc("The \"Include videos\" toggle")
    data class SetIncludeVideos(val on: Boolean) : BackupSettingsIntent

    @Serializable @SerialName("rename") @Doc("Type a new name for this phone and save it (1 to 64 characters). Not while a name is being saved")
    data class Rename(val name: String) : BackupSettingsIntent

    @Serializable @SerialName("back") @Doc("Back to the Backup screen")
    data object Back : BackupSettingsIntent
}

/**
 * Backup › Settings as an LS-DO screen ("backup_settings"). A wrapper for the same reasons as
 * [BackupLsDo]; [onBack] is the page's own back button.
 */
class BackupSettingsLsDo(private val vm: BackupSettingsViewModel, private val onBack: () -> Unit) :
    LsDoScreen<BackupSettingsView, BackupSettingsIntent> {
    override val lsdo = lsdoSpec<BackupSettingsView, BackupSettingsIntent>()

    override val state: StateFlow<BackupSettingsView> get() = vm.view

    override suspend fun perform(intent: BackupSettingsIntent): String? = when (intent) {
        is BackupSettingsIntent.SetAutoBackup -> done { vm.setAutoBackup(intent.on) }
        is BackupSettingsIntent.SetWifiOnly -> done { vm.setWifiOnly(intent.on) }
        is BackupSettingsIntent.SetChargingOnly -> done { vm.setChargingOnly(intent.on) }
        is BackupSettingsIntent.SetIncludeVideos -> done { vm.setIncludeVideos(intent.on) }
        is BackupSettingsIntent.Rename -> rename(intent.name)
        BackupSettingsIntent.Back -> done(onBack)
    }

    override fun availableIntents(state: BackupSettingsView): Set<String> = buildSet {
        addAll(listOf("set_auto_backup", "set_wifi_only", "set_charging_only", "set_include_videos", "back"))
        if (!state.renaming) add("rename")
    }

    override fun isBusy(state: BackupSettingsView): Boolean = state.renaming

    /** As typing in the field then "Save name": the server's answer comes back as the refusal. */
    private suspend fun rename(name: String): String? {
        vm.onNameChange(name)
        vm.saveName()
        return vm.state.first { !it.renaming }.nameError
    }
}

/** A call the screen cannot refuse. */
private inline fun done(call: () -> Unit): String? {
    call()
    return null
}
