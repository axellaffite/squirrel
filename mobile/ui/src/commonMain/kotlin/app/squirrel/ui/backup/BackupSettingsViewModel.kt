package app.squirrel.ui.backup

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.settings.BackupSettings
import app.squirrel.shared.settings.SettingsStore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class BackupSettingsState(
    val settings: BackupSettings = BackupSettings(),
    /** The phone's albums; null until loaded. */
    val albums: List<AlbumChoice>? = null,
    val albumsOpen: Boolean = false,
    val deviceName: String = "",
    val savedName: String = "",
    val nameError: String? = null,
    val renaming: Boolean = false,
) {
    val albumsLabel: String
        get() = when (val filter = settings.albums) {
            AlbumFilter.All -> "All photos"
            is AlbumFilter.Selected -> when (filter.names.size) {
                0 -> "No albums"
                1 -> albums?.firstOrNull { it.id == filter.names.first() }?.name ?: "1 album"
                else -> "${filter.names.size} albums"
            }
        }

    val folderHint: String get() = "Backups go to Photos/$savedName/<year>"
}

private const val MAX_NAME = 64

/** Backup › Settings: toggles, albums, this phone's name. Every change re-applies the schedule. */
class BackupSettingsViewModel(
    private val store: SettingsStore,
    private val applySchedule: () -> Unit,
    private val loadAlbums: suspend () -> List<AlbumChoice>,
    /** PATCH /api/devices/current; returns the name the server kept. */
    private val rename: suspend (String) -> String,
    deviceName: String,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    private val _state = MutableStateFlow(BackupSettingsState(deviceName = deviceName, savedName = deviceName))
    val state: StateFlow<BackupSettingsState> = _state.asStateFlow()

    init {
        viewModelScope.launch {
            val settings = withContext(io) { store.backup() }
            _state.update { it.copy(settings = settings) }
            val albums = withContext(io) { loadAlbums() } // names for the label
            _state.update { it.copy(albums = albums) }
        }
    }

    fun setAutoBackup(on: Boolean) = save { it.copy(autoBackup = on) }
    fun setWifiOnly(on: Boolean) = save { it.copy(wifiOnly = on) }
    fun setChargingOnly(on: Boolean) = save { it.copy(chargingOnly = on) }
    fun setIncludeVideos(on: Boolean) = save { it.copy(includeVideos = on) }

    fun openAlbums() {
        _state.update { it.copy(albumsOpen = true) }
        viewModelScope.launch {
            val albums = withContext(io) { loadAlbums() }
            _state.update { it.copy(albums = albums) }
        }
    }

    fun closeAlbums() = _state.update { it.copy(albumsOpen = false) }

    fun selectAllAlbums() = save { it.copy(albums = AlbumFilter.All) }

    /** From "All photos", choosing one album selects just that one. Albums are kept by id. */
    fun toggleAlbum(id: String) = save { current ->
        val selected = when (val filter = current.albums) {
            AlbumFilter.All -> setOf(id)
            is AlbumFilter.Selected -> if (id in filter.names) filter.names - id else filter.names + id
        }
        current.copy(albums = AlbumFilter.Selected(selected))
    }

    fun onNameChange(value: String) = _state.update { it.copy(deviceName = value, nameError = null) }

    fun saveName() {
        val name = _state.value.deviceName.trim()
        val error = when {
            name.isEmpty() -> "Enter a name for this phone"
            name.length > MAX_NAME -> "Use at most $MAX_NAME characters"
            else -> null
        }
        if (error != null) return _state.update { it.copy(nameError = error) }
        _state.update { it.copy(renaming = true) }
        viewModelScope.launch {
            try {
                val kept = rename(name)
                _state.update { it.copy(renaming = false, deviceName = kept, savedName = kept) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiError) {
                _state.update { it.copy(renaming = false, nameError = e.message ?: "The name was not saved") }
            } catch (_: Exception) {
                _state.update { it.copy(renaming = false, nameError = "Your Squirrel could not be reached") }
            }
        }
    }

    private fun save(change: (BackupSettings) -> BackupSettings) {
        val next = change(_state.value.settings)
        _state.update { it.copy(settings = next) }
        viewModelScope.launch {
            withContext(io) { store.setBackup(next) }
            applySchedule()
        }
    }
}
