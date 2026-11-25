package app.squirrel.ui.sharing

import app.squirrel.api.models.NodeKind
import app.squirrel.api.models.SharePermission
import app.squirrel.api.models.SharedItemDto
import app.squirrel.ui.files.OpenerState
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** What a shared row is, which decides where open takes it. */
@Serializable
enum class SharedKind {
    @SerialName("folder") Folder,
    @SerialName("file") File,
    @SerialName("photo") Photo,
    @SerialName("album") Album,
}

@Serializable
data class SharedRow(
    @Doc("The share's id: what open and ask_leave take") val id: String,
    val kind: SharedKind,
    val name: String,
    @Doc("The folder, file or album's own id (an open folder is the shared screen's folderId)") val subjectId: String,
    @Doc("Who shared it") val owner: String,
    @Doc("You may edit it (\"Can edit\"); else view only") val canEdit: Boolean,
    @Doc("Photos in an album; null for folders and files") val photoCount: Long? = null,
)

@Serializable
data class SharedListState(
    @Doc("What other people shared with you, newest first; null until the first load") val items: List<SharedRow>? = null,
    val loading: Boolean = false,
    val error: String? = null,
    @Doc("The share id waiting for \"Leave …?\": confirm_leave or cancel_leave") val leaving: String? = null,
    @Doc("What became of a file a person opened with another app") val message: String? = null,
)

/** What an agent can do on the Shared list (LS-DO); each maps to the call the list makes. */
@Serializable
sealed interface SharedListIntent {
    @Serializable @SerialName("open")
    @Doc("Open a row by its id in items: a folder opens in the shared browser (screen shared), an album in its page, a photo in the viewer. Not a file: it opens in another app")
    data class Open(val id: String) : SharedListIntent

    @Serializable @SerialName("refresh") @Doc("Reload the list, as pulling down does; also tries again after an error")
    data object Refresh : SharedListIntent

    @Serializable @SerialName("ask_leave") @Doc("Leave a share by its id in items: asks first, as the Leave action does")
    data class AskLeave(val id: String) : SharedListIntent

    @Serializable @SerialName("confirm_leave") @Doc("Answer yes to \"Leave …?\": the share goes from the list. Only while it is asked (leaving set)")
    data object ConfirmLeave : SharedListIntent

    @Serializable @SerialName("cancel_leave") @Doc("Answer no to \"Leave …?\". Only while it is asked (leaving set)")
    data object CancelLeave : SharedListIntent
}

fun sharedKind(item: SharedItemDto): SharedKind {
    val node = item.node ?: return SharedKind.Album
    return when {
        node.kind == NodeKind.folder -> SharedKind.Folder
        isPhotoFile(node.mime) -> SharedKind.Photo
        else -> SharedKind.File
    }
}

/** The list as an agent reads it: the rows the screen shows, without covers or dates. */
internal fun sharedListState(state: SharedState, opener: OpenerState): SharedListState = SharedListState(
    items = state.items?.map { item ->
        SharedRow(
            id = item.shareId,
            kind = sharedKind(item),
            name = subjectName(item),
            subjectId = item.node?.id ?: item.album?.id.orEmpty(),
            owner = memberName(item.owner),
            canEdit = item.permission == SharePermission.write,
            photoCount = item.album?.itemCount,
        )
    },
    loading = state.loading,
    error = state.error,
    leaving = state.leaving?.shareId,
    message = opener.message,
)

/**
 * The Shared list as an LS-DO screen ("shared_list"). Opening navigates, which the tab owns:
 * [open] is the tab's own open, as tapping a row calls it.
 */
class SharedListLsDo(private val vm: SharedViewModel, private val open: (SharedItemDto) -> Unit) : LsDoScreen<SharedListState, SharedListIntent> {
    override val lsdo = lsdoSpec<SharedListState, SharedListIntent>()
    override val state: StateFlow<SharedListState> = vm.listing

    override suspend fun perform(intent: SharedListIntent): String? = when (intent) {
        is SharedListIntent.Open -> withItem(intent.id) { item ->
            // A file goes to another app, which an agent cannot see or drive.
            if (sharedKind(item) == SharedKind.File) "\"${subjectName(item)}\" is a file: it opens in another app, which LS-DO does not drive."
            else done { open(item) }
        }
        SharedListIntent.Refresh -> done { vm.load() }
        is SharedListIntent.AskLeave -> withItem(intent.id) { done { vm.askLeave(it) } }
        SharedListIntent.ConfirmLeave -> vm.leave()
        SharedListIntent.CancelLeave -> done { vm.cancelLeave() }
    }

    override fun availableIntents(state: SharedListState): Set<String> = buildSet {
        add("refresh")
        if (!state.items.isNullOrEmpty()) addAll(listOf("open", "ask_leave"))
        if (state.leaving != null) addAll(listOf("confirm_leave", "cancel_leave"))
    }

    override fun isBusy(state: SharedListState): Boolean = state.loading

    /** Runs [action] on the shared item [id] of the list, or says it is not there. */
    private inline fun withItem(id: String, action: (SharedItemDto) -> String?): String? {
        val item = vm.state.value.items.orEmpty().firstOrNull { it.shareId == id } ?: return "No share \"$id\" in the list."
        return action(item)
    }
}

/** A call the screen cannot refuse. */
private inline fun done(call: () -> Unit): String? {
    call()
    return null
}
