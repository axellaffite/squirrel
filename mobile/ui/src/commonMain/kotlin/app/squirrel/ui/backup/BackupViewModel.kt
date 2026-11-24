package app.squirrel.ui.backup

import app.squirrel.ui.combineState
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.BackupStatus
import app.squirrel.shared.sync.MediaAccess
import app.squirrel.shared.sync.QueuedAsset
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

/** The status (photo access filled in) and what the screen makes of it. */
internal class BackupSnapshot(val status: BackupStatus, val ui: BackupUi)

/** The Backup screen: status, "Back up now", failures. */
class BackupViewModel(
    status: Flow<BackupStatus>,
    settings: Flow<BackupSettings>,
    private val locationsIncluded: () -> Boolean,
    private val access: () -> MediaAccess,
    private val now: () -> Long,
    private val onBackUpNow: () -> Unit,
    private val failures: suspend () -> List<QueuedAsset>,
    private val batteryRestricted: () -> Boolean = { false },
) : ViewModel() {
    /** Bumped on resume: partial access, or the battery setting, can change while the app is away. */
    private val refresh = MutableStateFlow(0)

    private val snapshot: StateFlow<BackupSnapshot?> = combine(status, settings, refresh) { s, prefs, _ ->
        val current = s.copy(access = access())
        BackupSnapshot(current, backupUi(current, prefs, locationsIncluded(), now(), batteryRestricted()))
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_TIMEOUT), null)

    val ui: StateFlow<BackupUi?> = snapshot.map { it?.ui }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_TIMEOUT), null)

    private val _failed = MutableStateFlow<List<QueuedAsset>?>(null)

    /** The failure sheet's rows while it is open. */
    val failed: StateFlow<List<QueuedAsset>?> = _failed

    /**
     * What an agent reads (LS-DO). Started on first read, which only a debug build's agent
     * makes, then kept up while this model lives: the registry reads it without collecting.
     */
    internal val view: StateFlow<BackupView> by lazy {
        combineState(snapshot, _failed) { s, failed -> backupView(s, failed) }
    }

    fun onResume() {
        refresh.value++
    }

    fun backUpNow() = onBackUpNow()

    suspend fun showFailures() {
        _failed.value = failures()
    }

    fun hideFailures() {
        _failed.value = null
    }

    fun retryFailures() {
        _failed.value = null
        onBackUpNow()
    }

    private companion object {
        const val STOP_TIMEOUT = 5_000L
    }
}
