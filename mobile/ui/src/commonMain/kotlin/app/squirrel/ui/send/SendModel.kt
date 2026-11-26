package app.squirrel.ui.send

import app.squirrel.api.models.CreateFolderRequest
import app.squirrel.api.models.CreateShareRequest
import app.squirrel.api.models.FriendDto
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.NodeKind
import app.squirrel.api.models.PathSegmentDto
import app.squirrel.api.models.SharePermission
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.sync.FileUpload
import app.squirrel.ui.files.PickedFile
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.call
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** The top-level folder the default destination lives in, and the folder itself. */
const val SENT_ROOT = "Files"
const val SENT_FOLDER = "Sent"

/** Folders listed per request while looking for [SENT_FOLDER]. */
private const val FOLDER_PAGE = 500

enum class SendPhase { Editing, Sending, Sent }

data class SendState(
    /** The files handed over, by name. */
    val names: List<String> = emptyList(),
    /** Null until loaded (or when the server has no friends list: typing still works). */
    val friends: List<FriendDto>? = null,
    /** The friend tapped, by username. */
    val picked: String? = null,
    /** A username typed instead. */
    val typed: String = "",
    /** The folder chosen, top level first; null: [SENT_ROOT] › [SENT_FOLDER], created when missing. */
    val folder: List<PathSegmentDto>? = null,
    val phase: SendPhase = SendPhase.Editing,
    /** 0 to 1 over every file, while sending. */
    val progress: Float = 0f,
    val error: String? = null,
    /** What happened once sent ("Sent 2 files to Anna."). */
    val notice: String? = null,
) {
    /** Who gets the files: the friend tapped, else the username typed. */
    val recipient: String? get() = picked ?: typed.trim().takeIf { it.isNotEmpty() }
    val canSend: Boolean get() = recipient != null && phase == SendPhase.Editing && names.isNotEmpty()
    val folderLabel: String get() = folder?.joinToString(" › ") { it.name } ?: "$SENT_ROOT › $SENT_FOLDER"
}

/** Puts one file into a folder: the new node. Throws on failure (its message may be shown). */
typealias Uploader = suspend (parentId: String, file: PickedFile, onProgress: (Float) -> Unit) -> NodeDto

/** [FileUpload], reading the file the way [app.squirrel.ui.files.UploadsViewModel] does. */
fun fileUploader(api: SquirrelApi, upload: FileUpload = FileUpload(api)): Uploader = { parentId, file, onProgress ->
    val reader = file.open() ?: throw UnreadableFile()
    reader.use { upload.upload(parentId, file.name, it, onProgress) }
}

class UnreadableFile : Exception("it could not be read on this phone.")

/**
 * "Send with Squirrel": files another app handed over go into a folder of one's own (Files › Sent
 * unless another is chosen), then each is shared, read only, with one person, a friend or a
 * username. Files are uploaded and shared one at a time, and the first failure stops the rest.
 */
class SendModel(
    private val api: SquirrelApi,
    private val files: List<PickedFile>,
    private val scope: CoroutineScope,
    private val upload: Uploader = fileUploader(api),
) {
    private val _state = MutableStateFlow(SendState(names = files.map { it.name }))
    val state: StateFlow<SendState> = _state.asStateFlow()

    fun load() {
        scope.launch {
            val result = call { api.friends.listFriends().bodyOrThrow().friends }
            if (result is Outcome.Ok) _state.update { it.copy(friends = result.value) }
        }
    }

    fun pick(username: String) = _state.update { it.copy(picked = username, typed = "", error = null) }

    fun type(value: String) = _state.update { it.copy(typed = value, picked = null, error = null) }

    /** The folder to send into, top level first; null: back to Files › Sent. */
    fun choose(trail: List<PathSegmentDto>?) = _state.update { it.copy(folder = trail?.takeIf { t -> t.isNotEmpty() }) }

    fun send() {
        val start = _state.value
        val who = start.recipient ?: return _state.update { it.copy(error = "Choose who to send it to.") }
        if (start.phase != SendPhase.Editing) return
        _state.update { it.copy(phase = SendPhase.Sending, progress = 0f, error = null) }
        scope.launch {
            val problem = try {
                deliver(who, start)
            } finally {
                files.forEach { it.release() }
            }
            _state.update { s ->
                if (problem == null) {
                    s.copy(phase = SendPhase.Sent, progress = 1f, notice = sentNotice(files.size, nameOf(who, s.friends)))
                } else {
                    s.copy(phase = SendPhase.Editing, error = problem)
                }
            }
        }
    }

    /** Null once every file is in and shared, else what went wrong. */
    private suspend fun deliver(who: String, start: SendState): String? {
        val folderId = when (val target = call { start.folder?.last()?.id ?: sentFolder() }) {
            is Outcome.Ok -> target.value
            is Outcome.Failed -> return "The folder ${start.folderLabel} could not be opened: ${target.message}"
        }
        files.forEachIndexed { n, file ->
            val node = try {
                upload(folderId, file) { p -> _state.update { it.copy(progress = (n + p) / files.size) } }
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                return "${file.name} could not be sent: ${uploadProblem(e)}"
            }
            val shared = call {
                api.sharing.createShare(CreateShareRequest(permission = SharePermission.read, username = who, nodeId = node.id)).bodyOrThrow()
            }
            if (shared is Outcome.Failed) return "${file.name} was saved in ${start.folderLabel} but not shared: ${shared.message}"
        }
        return null
    }

    /** Files › Sent: found by name in the top-level Files folder, or created there. */
    private suspend fun sentFolder(): String {
        val roots = api.files.listNodes(parentId = null).bodyOrThrow().items
        val root = roots.firstOrNull { it.name == SENT_ROOT } ?: roots.first()
        var cursor: String? = null
        do {
            val page = api.files.listNodes(parentId = root.id, cursor = cursor, limit = FOLDER_PAGE).bodyOrThrow()
            page.items.firstOrNull { it.kind == NodeKind.folder && it.name == SENT_FOLDER }?.let { return it.id }
            cursor = page.nextCursor
        } while (cursor != null)
        return api.files.createFolder(CreateFolderRequest(name = SENT_FOLDER, parentId = root.id)).bodyOrThrow().id
    }
}

private fun uploadProblem(e: Exception): String = when (e) {
    is UnreadableFile -> e.message.orEmpty()
    is app.squirrel.shared.api.ApiError -> if (e.fromServer) e.message.orEmpty() else app.squirrel.ui.photos.UNREACHABLE
    else -> app.squirrel.ui.photos.UNREACHABLE
}

/** A friend's display name, else the username. */
private fun nameOf(username: String, friends: List<FriendDto>?): String =
    friends?.firstOrNull { it.username == username }?.displayName ?: username

fun sentNotice(count: Int, to: String): String = if (count == 1) "Sent 1 file to $to." else "Sent $count files to $to."
