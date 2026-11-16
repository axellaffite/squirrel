package app.squirrel.ui.backup

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.sync.BackupStatus
import app.squirrel.shared.sync.QueuedAsset
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn

/** The Backup screen: status, "Back up now", failures. */
class BackupViewModel(
    status: Flow<BackupStatus>,
    settings: Flow<BackupSettings>,
    private val locationsIncluded: () -> Boolean,
    private val access: () -> app.squirrel.shared.sync.MediaAccess,
    private val now: () -> Long,
    private val onBackUpNow: () -> Unit,
    private val failures: suspend () -> List<QueuedAsset>,
) : ViewModel() {
    /** Bumped on resume: partial access can shrink or expire while the app is away. */
    private val refresh = MutableStateFlow(0)

    val ui: StateFlow<BackupUi?> = combine(status, settings, refresh) { s, prefs, _ ->
        backupUi(s.copy(access = access()), prefs, locationsIncluded(), now())
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_TIMEOUT), null)

    private val _failed = MutableStateFlow<List<QueuedAsset>?>(null)

    /** The failure sheet's rows while it is open. */
    val failed: StateFlow<List<QueuedAsset>?> = _failed

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
