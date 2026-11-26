package app.squirrel.ui.photos

import app.squirrel.api.models.AccessLevel
import app.squirrel.api.models.AlbumDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.files.albumNameProblem
import app.squirrel.ui.sharing.Abilities
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.memberName
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** An album as LS-DO lists it. */
@Serializable
data class AlbumRow(
    val id: String,
    val name: String,
    @Doc("Photos in it") val count: Long,
    @Doc("What you may do with it: owner, write (add and remove your own photos) or read") val access: AccessLevel,
    @Doc("Who shared it with you; null: your own") val sharedBy: String? = null,
    @Doc("Its photos come from rules: none are added or removed by hand") val dynamic: Boolean = false,
)

private fun AlbumDto.row() = AlbumRow(id, name, itemCount, access, sharedBy?.let(::memberName), dynamic)

@Serializable
data class AlbumListView(
    @Doc("Null until the list first arrives") val albums: List<AlbumRow>? = null,
    val loading: Boolean = false,
    @Doc("Loading again over the albums shown") val refreshing: Boolean = false,
    val error: String? = null,
)

/** What an agent can do on the Albums sub-tab (LS-DO). */
@Serializable
sealed interface AlbumsIntent {
    @Serializable @SerialName("open") @Doc("Open an album (by its id in albums) on its own page")
    data class Open(val id: String) : AlbumsIntent

    @Serializable @SerialName("create") @Doc("New album, as the + button does; its page opens once created")
    data class Create(val name: String) : AlbumsIntent

    @Serializable @SerialName("new_dynamic") @Doc("Open the rule editor of a new dynamic album (album_rules), as + then Dynamic album does")
    data object NewDynamic : AlbumsIntent

    @Serializable @SerialName("refresh") @Doc("Reload the list, as pulling down does")
    data object Refresh : AlbumsIntent

    @Serializable @SerialName("retry") @Doc("Try the failed load again, after an error")
    data object Retry : AlbumsIntent
}

/** The Albums sub-tab as an LS-DO screen ("albums"). [onOpen] opens an album's page as a card tap does. */
class AlbumsLsDo(
    private val vm: AlbumsViewModel,
    private val api: SquirrelApi,
    scope: CoroutineScope,
    private val onNewDynamic: () -> Unit = {},
    private val onOpen: (AlbumDto) -> Unit,
) : LsDoScreen<AlbumListView, AlbumsIntent> {
    override val lsdo = lsdoSpec<AlbumListView, AlbumsIntent>()

    override val state: StateFlow<AlbumListView> = vm.state.view { s ->
        AlbumListView(s.albums?.map { it.row() }, s.loading, s.refreshing, s.error)
    }

    override suspend fun perform(intent: AlbumsIntent): String? = when (intent) {
        is AlbumsIntent.Open -> {
            val album = vm.state.value.albums.orEmpty().firstOrNull { it.id == intent.id } ?: return "No album \"${intent.id}\" in the list."
            done { onOpen(album) }
        }
        is AlbumsIntent.Create -> create(intent.name)
        AlbumsIntent.NewDynamic -> done(onNewDynamic)
        AlbumsIntent.Refresh -> done { vm.refresh() }
        AlbumsIntent.Retry -> done { vm.load() }
    }

    /** What the "New album" sheet does: check the name, create, refresh the list, open it. */
    private suspend fun create(name: String): String? {
        albumNameProblem(name)?.let { return it }
        return when (val result = createAlbum(api, name.trim())) {
            is Outcome.Ok -> done {
                vm.refresh()
                onOpen(result.value)
            }
            is Outcome.Failed -> result.message
        }
    }

    override fun availableIntents(state: AlbumListView): Set<String> = buildSet {
        addAll(listOf("create", "new_dynamic", "refresh"))
        if (!state.albums.isNullOrEmpty()) add("open")
        if (state.error != null) add("retry")
    }

    override fun isBusy(state: AlbumListView): Boolean = state.loading
}

@Serializable
data class AlbumView(
    val id: String,
    @Doc("The album's name (as opened, until its details arrive)") val name: String,
    @Doc("Photos in it; null until its details arrive") val count: Long? = null,
    @Doc("What you may do: owner (rename, set the cover, remove), write (remove your own photos) or read; null until its details arrive") val access: AccessLevel? = null,
    @Doc("Who shared it with you; null: your own") val sharedBy: String? = null,
    @Doc("Its photos come from rules (no remove; the owner edits them with edit_rules)") val dynamic: Boolean = false,
    @Doc("Its photos loaded so far, page by page (more)") val photos: List<PhotoRow> = emptyList(),
    @Doc("Every photo of it is loaded") val complete: Boolean = false,
    val loading: Boolean = false,
    val refreshing: Boolean = false,
    val error: String? = null,
    @Doc("The photo open in the viewer") val viewing: String? = null,
    @Doc("What the last change did") val notice: String? = null,
    @Doc("The notice is an error") val noticeIsError: Boolean = false,
)

/** What an agent can do on one album's page (LS-DO); back to the list is photos back. */
@Serializable
sealed interface AlbumIntent {
    @Serializable @SerialName("more") @Doc("Load the next page of photos, as scrolling to the end does. Not once complete")
    data object More : AlbumIntent

    @Serializable @SerialName("refresh") @Doc("Reload the album from its first page, as pulling down does")
    data object Refresh : AlbumIntent

    @Serializable @SerialName("retry") @Doc("Try the failed load again, after an error")
    data object Retry : AlbumIntent

    @Serializable @SerialName("open") @Doc("Open a loaded photo (by its id in photos) in the viewer. Not while one is open")
    data class Open(val id: String) : AlbumIntent

    @Serializable @SerialName("close_photo") @Doc("Close the viewer. Only while a photo is open (viewing set)")
    data object ClosePhoto : AlbumIntent

    @Serializable @SerialName("rename") @Doc("Rename the album, as the pencil does. Owner only")
    data class Rename(val name: String) : AlbumIntent

    @Serializable @SerialName("set_cover") @Doc("Make a loaded photo the album's cover, as Set as cover does. Owner only")
    data class SetCover(@Doc("A photo id from photos") val id: String) : AlbumIntent

    @Serializable @SerialName("remove")
    @Doc("Take loaded photos out of the album (they stay in the timeline), as selecting them and Remove does. Owner or write access; with write, only the photos you added go")
    data class Remove(@Doc("Photo ids from photos") val ids: List<String>) : AlbumIntent

    @Serializable @SerialName("edit_rules") @Doc("Open the rule editor (album_rules), as the sliders button does. Owner of a dynamic album only")
    data object EditRules : AlbumIntent
}

/**
 * One album's page as an LS-DO screen ("album"). The viewer and the notice stay Compose state,
 * mirrored in [overlay]; [onNotice] shows a change's outcome as the page's own buttons do.
 */
class AlbumLsDo(
    private val vm: AlbumViewModel,
    private val api: SquirrelApi,
    private val albumId: String,
    name: String,
    scope: CoroutineScope,
    private val onOpen: (String) -> Unit,
    private val onClose: () -> Unit,
    private val onEditRules: () -> Unit = {},
    private val onNotice: (String, Boolean) -> Unit,
) : LsDoScreen<AlbumView, AlbumIntent> {
    val overlay = OverlayMirror()

    override val lsdo = lsdoSpec<AlbumView, AlbumIntent>()

    override val state: StateFlow<AlbumView> = vm.state.viewWith(overlay.state) { s, o ->
        val album = s.album
        AlbumView(
            id = albumId,
            name = album?.name ?: name,
            count = album?.itemCount,
            access = album?.access,
            sharedBy = album?.sharedBy?.let(::memberName),
            dynamic = album?.dynamic == true,
            photos = s.items.map { it.row() },
            complete = s.complete,
            loading = s.loading,
            refreshing = s.refreshing,
            error = s.error,
            viewing = o.viewing,
            notice = o.notice,
            noticeIsError = o.noticeIsError,
        )
    }

    private val shown get() = vm.state.value.items

    override suspend fun perform(intent: AlbumIntent): String? = when (intent) {
        AlbumIntent.More -> done { vm.more() }
        AlbumIntent.Refresh -> done { vm.refresh() }
        AlbumIntent.Retry -> done { vm.retry() }
        is AlbumIntent.Open -> missingPhotos(listOf(intent.id), shown, "this album") ?: done { onOpen(intent.id) }
        AlbumIntent.ClosePhoto -> done(onClose)
        is AlbumIntent.Rename -> rename(intent.name)
        is AlbumIntent.SetCover -> missingPhotos(listOf(intent.id), shown, "this album") ?: setCover(intent.id)
        is AlbumIntent.Remove ->
            if (state.value.dynamic) "A dynamic album's photos come from its rules: none can be removed." else missingPhotos(intent.ids, shown, "this album") ?: remove(intent.ids)
        AlbumIntent.EditRules -> if (state.value.dynamic && state.value.access == AccessLevel.owner) done(onEditRules) else "Only the owner of a dynamic album edits its rules."
    }

    /** What the rename sheet does: check the name, save it, read the album again. */
    private suspend fun rename(name: String): String? {
        albumNameProblem(name)?.let { return it }
        return when (val result = renameAlbum(api, albumId, name.trim())) {
            is Outcome.Ok -> done { vm.details() }
            is Outcome.Failed -> result.message
        }
    }

    private suspend fun setCover(photoId: String): String? = when (val result = setAlbumCover(api, albumId, photoId)) {
        is Outcome.Ok -> done {
            onNotice("Cover changed.", false)
            vm.details()
        }
        is Outcome.Failed -> result.message.also { onNotice(it, true) }
    }

    private suspend fun remove(ids: List<String>): String? {
        val result = removeFromAlbum(api, albumId, ids)
        vm.refresh()
        return when (result) {
            is Outcome.Ok -> done { onNotice(removedMessage(result.value), result.value == 0L) }
            is Outcome.Failed -> result.message.also { onNotice(it, true) }
        }
    }

    override fun availableIntents(state: AlbumView): Set<String> = buildSet {
        add("refresh")
        if (!state.complete) add("more")
        if (state.error != null) add("retry")
        if (state.viewing == null) add("open") else add("close_photo")
        val can = Abilities.of(state.access ?: AccessLevel.read)
        if (can.owner) addAll(listOf("rename", "set_cover"))
        if (can.edit && !state.dynamic) add("remove")
        if (can.owner && state.dynamic) add("edit_rules")
    }

    override fun isBusy(state: AlbumView): Boolean = state.loading || state.refreshing
}
