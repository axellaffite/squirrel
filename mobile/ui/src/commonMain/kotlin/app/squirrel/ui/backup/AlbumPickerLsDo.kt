package app.squirrel.ui.backup

import app.squirrel.lsdo.Doc
import app.squirrel.lsdo.LsDoScreen
import app.squirrel.lsdo.lsdoSpec
import app.squirrel.shared.settings.AlbumFilter
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** What the phone groups photos in. */
@Serializable
enum class AlbumGroup {
    @SerialName("albums") Albums,
    @SerialName("folders") Folders,
}

@Serializable
data class AlbumRow(
    @Doc("What toggle takes") val id: String,
    val name: String,
    @Doc("Photos and videos in it") val count: Int,
    @Doc("Where it lives (Android)") val path: String?,
    @Doc("Backed up (false for every row while all photos are)") val selected: Boolean,
)

/** The album (iOS) or folder (Android) choice as an agent reads it. */
@Serializable
data class AlbumPickerView(
    @Doc("The sheet is open: only then can the choice change") val open: Boolean,
    @Doc("Albums (iOS) or folders (Android)") val kind: AlbumGroup,
    @Doc("Every photo is backed up, whatever the album") val all: Boolean,
    @Doc("The ids backed up when not all (they may include albums no longer on the phone)") val selected: List<String>,
    @Doc("The phone's albums or folders; null until loaded") val albums: List<AlbumRow>?,
    @Doc("What the Albums/Folders row shows: \"All photos\", \"Camera, Screenshots\"") val label: String,
)

internal fun albumPickerView(state: AlbumPickerState): AlbumPickerView {
    val chosen = (state.filter as? AlbumFilter.Selected)?.names.orEmpty()
    return AlbumPickerView(
        open = state.open,
        kind = if (state.kind == AlbumKind.Folders) AlbumGroup.Folders else AlbumGroup.Albums,
        all = state.filter == AlbumFilter.All,
        selected = chosen.sorted(),
        albums = state.albums?.map { AlbumRow(it.id, it.name, it.count, it.path, it.id in chosen) },
        label = state.label,
    )
}

/** What an agent can do with the album choice (LS-DO); each maps to the [AlbumPickerViewModel] call the UI makes. */
@Serializable
sealed interface AlbumPickerIntent {
    @Serializable @SerialName("open") @Doc("Open the sheet, as tapping the Albums/Folders row does; it reloads the list")
    data object Open : AlbumPickerIntent

    @Serializable @SerialName("close") @Doc("Close the sheet. Only while it is open")
    data object Close : AlbumPickerIntent

    @Serializable @SerialName("select_all") @Doc("Back up all photos. Only while the sheet is open")
    data object SelectAll : AlbumPickerIntent

    @Serializable @SerialName("toggle") @Doc("Add or remove one album (by its id in albums); from all photos, it selects just that one. Only while the sheet is open and the list loaded")
    data class Toggle(val id: String) : AlbumPickerIntent
}

/**
 * The album choice as an LS-DO screen ("album_picker"), shown wherever its row is (Backup and
 * Backup › Settings). A wrapper: the model's own state holds a filter that is not serializable.
 */
class AlbumPickerLsDo(private val vm: AlbumPickerViewModel) : LsDoScreen<AlbumPickerView, AlbumPickerIntent> {
    override val lsdo = lsdoSpec<AlbumPickerView, AlbumPickerIntent>()

    override val state: StateFlow<AlbumPickerView> get() = vm.view

    override suspend fun perform(intent: AlbumPickerIntent): String? = when (intent) {
        AlbumPickerIntent.Open -> done { vm.open() }
        AlbumPickerIntent.Close -> done { vm.close() }
        AlbumPickerIntent.SelectAll -> done { vm.selectAll() }
        is AlbumPickerIntent.Toggle -> toggle(intent.id)
    }

    override fun availableIntents(state: AlbumPickerView): Set<String> = when {
        !state.open -> setOf("open")
        state.albums == null -> setOf("close", "select_all")
        else -> setOf("close", "select_all", "toggle")
    }

    override fun isBusy(state: AlbumPickerView): Boolean = state.open && state.albums == null

    private fun toggle(id: String): String? {
        val shown = vm.state.value
        if (shown.albums.orEmpty().none { it.id == id }) return "No ${shown.kind.one} \"$id\" on this phone."
        vm.toggle(id)
        return null
    }
}

/** A call the screen cannot refuse. */
private inline fun done(call: () -> Unit): String? {
    call()
    return null
}
