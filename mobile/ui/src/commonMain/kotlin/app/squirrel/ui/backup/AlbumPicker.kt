package app.squirrel.ui.backup

import app.squirrel.ui.mapState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import app.squirrel.shared.settings.AlbumFilter
import app.squirrel.shared.settings.SettingsStore
import app.squirrel.shared.sync.formatCount
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.components.Icon
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.Sheet
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** How many chosen names the label spells out before "and 3 more". */
private const val NAMED = 2

/**
 * What is backed up, in a few words: "All photos", "Camera, Screenshots", "Camera, Screenshots
 * and 3 more". Before the names are known (or for ids no longer on the phone), a count.
 */
fun albumsLabel(filter: AlbumFilter, albums: List<AlbumChoice>?, kind: AlbumKind): String = when (filter) {
    AlbumFilter.All -> "All photos"
    is AlbumFilter.Selected -> {
        val known = albums.orEmpty().filter { it.id in filter.names }.map { it.name }
        val total = filter.names.size
        when {
            total == 0 -> "No ${kind.many}"
            known.isEmpty() -> "$total ${if (total == 1) kind.one else kind.many}"
            else -> {
                val shown = known.take(NAMED)
                val rest = total - shown.size
                shown.joinToString(", ") + if (rest > 0) " and $rest more" else ""
            }
        }
    }
}

/** Under a choice's name: where it lives (Android) and how many photos and videos it holds. */
fun albumMeta(album: AlbumChoice): String = listOfNotNull(album.path, formatCount(album.count.toLong())).joinToString(" · ")

data class AlbumPickerState(
    val filter: AlbumFilter = AlbumFilter.All,
    /** The phone's albums or folders; null until loaded. */
    val albums: List<AlbumChoice>? = null,
    val open: Boolean = false,
    val kind: AlbumKind = AlbumKind.Albums,
) {
    val label: String get() = albumsLabel(filter, albums, kind)
}

/**
 * Which albums (iOS) or folders (Android) are backed up: one instance behind both the Backup
 * screen's row and Backup › Settings, so both show and change the same choice. Kept by id in
 * the settings store; every change re-applies the schedule.
 */
class AlbumPickerViewModel(
    private val store: SettingsStore,
    private val applySchedule: () -> Unit,
    private val loadAlbums: suspend () -> List<AlbumChoice>,
    kind: AlbumKind,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    private val _state = MutableStateFlow(AlbumPickerState(kind = kind))
    val state: StateFlow<AlbumPickerState> = _state.asStateFlow()

    /** What an agent reads (LS-DO); started on first read, as [BackupViewModel.view]. */
    internal val view: StateFlow<AlbumPickerView> by lazy {
        state.mapState(::albumPickerView)
    }

    init {
        viewModelScope.launch {
            val filter = withContext(io) { store.backup().albums }
            _state.update { it.copy(filter = filter) }
            reload() // names for the label
        }
    }

    fun open() {
        _state.update { it.copy(open = true) }
        viewModelScope.launch { reload() }
    }

    fun close() = _state.update { it.copy(open = false) }

    fun selectAll() = save(AlbumFilter.All)

    /** From "All photos", choosing one selects just that one. */
    fun toggle(id: String) {
        val selected = when (val filter = _state.value.filter) {
            AlbumFilter.All -> setOf(id)
            is AlbumFilter.Selected -> if (id in filter.names) filter.names - id else filter.names + id
        }
        save(AlbumFilter.Selected(selected))
    }

    private suspend fun reload() {
        val albums = withContext(io) { loadAlbums() }
        _state.update { it.copy(albums = albums) }
    }

    private fun save(filter: AlbumFilter) {
        _state.update { it.copy(filter = filter) }
        viewModelScope.launch {
            // Only the album choice: the other settings may have changed on the settings screen.
            withContext(io) { store.setBackup(store.backup().copy(albums = filter)) }
            applySchedule()
        }
    }
}

/** The sheet listing "All photos" then each album or folder, with a check on what is backed up. */
@Composable
fun AlbumPickerSheet(vm: AlbumPickerViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    if (!state.open) return
    val type = SquirrelTheme.type
    val kind = state.kind
    Sheet(kind.title, onDismiss = vm::close) {
        val filter = state.filter
        ListRow("All photos", icon = SquirrelIcon.Images, onClick = vm::selectAll, trailing = {
            if (filter == AlbumFilter.All) Selected()
        })
        val albums = state.albums
        if (albums == null) {
            BasicText("Loading ${kind.many}…", style = type.caption)
        } else {
            albums.forEach { album ->
                val selected = filter is AlbumFilter.Selected && album.id in filter.names
                ListRow(album.name, icon = kind.icon, meta = albumMeta(album), onClick = { vm.toggle(album.id) }, trailing = {
                    if (selected) Selected()
                })
            }
        }
        BasicText("Changing ${kind.many} checks your whole library again; nothing is deleted from your Squirrel.", style = type.caption)
    }
}

@Composable
private fun Selected() {
    Icon(SquirrelIcon.Check, contentDescription = "Selected", tint = SquirrelTheme.scheme.accent)
}
