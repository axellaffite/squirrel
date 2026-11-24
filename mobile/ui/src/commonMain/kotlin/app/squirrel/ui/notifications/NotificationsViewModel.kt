package app.squirrel.ui.notifications

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.MarkReadRequest
import app.squirrel.api.models.NodeDto
import app.squirrel.api.models.NotificationDto
import app.squirrel.api.models.NotificationSubjectDto
import app.squirrel.api.models.SubjectKind
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.call
import app.squirrel.ui.sharing.isPhotoFile
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class NotificationsState(
    val unread: Long = 0,
    /** The sheet is shown. */
    val open: Boolean = false,
    /** Null until the first page arrived; newest first. */
    val items: List<NotificationDto>? = null,
    /** The next page's cursor; null when everything is shown. */
    val cursor: String? = null,
    val loading: Boolean = false,
    val loadingMore: Boolean = false,
    /** Marking read, waiting for the server. */
    val saving: Boolean = false,
    val error: String? = null,
)

/** Where tapping a notification goes, in the Shared section's pages. */
sealed interface NotificationTarget {
    data class Folder(val id: String) : NotificationTarget
    data class Album(val id: String, val name: String) : NotificationTarget
    data class Photo(val id: String, val name: String) : NotificationTarget

    /** A file shared on its own that is not a photo: it opens with another app. */
    data class File(val id: String) : NotificationTarget

    /** [File] once its details arrived: what the Shared section hands to another app. */
    data class FileNode(val node: NodeDto) : NotificationTarget
}

/** A file opens its folder when the caller may see it, else as the Shared list opens a file shared alone. */
fun targetOf(subject: NotificationSubjectDto): NotificationTarget = when (subject.kind) {
    SubjectKind.album -> NotificationTarget.Album(subject.id, subject.name)
    SubjectKind.folder -> NotificationTarget.Folder(subject.id)
    else -> subject.folderId?.let { NotificationTarget.Folder(it) }
        ?: if (isPhotoFile(subject.mime)) NotificationTarget.Photo(subject.id, subject.name) else NotificationTarget.File(subject.id)
}

/** "9+" style count for the bell. */
fun unreadBadge(unread: Long): String? = when {
    unread <= 0 -> null
    unread > MAX_BADGE -> "$MAX_BADGE+"
    else -> unread.toString()
}

private const val MAX_BADGE = 99

/** The inbox: the bell's unread count, the list, reading. */
class NotificationsViewModel(private val api: SquirrelApi) : ViewModel() {
    private val _state = MutableStateFlow(NotificationsState())
    val state: StateFlow<NotificationsState> = _state.asStateFlow()

    /** The app came back or the shell showed: the count, and the list if it is open. */
    fun resumed() {
        refreshCount()
        if (_state.value.open) load()
    }

    /** Bumped by each marking: a count asked for before it is stale once it arrives. */
    private var marks = 0

    fun refreshCount() {
        val asked = marks
        viewModelScope.launch {
            val result = call { api.notifications.unreadCount().bodyOrThrow() }
            if (result is Outcome.Ok && asked == marks) _state.update { it.copy(unread = result.value.unread) }
        }
    }

    fun open() {
        _state.update { it.copy(open = true) }
        load()
    }

    fun close() = _state.update { it.copy(open = false, error = null) }

    /** The first page, again. */
    fun load() {
        if (_state.value.loading) return
        _state.update { it.copy(loading = true) }
        viewModelScope.launch {
            val result = call { api.notifications.listNotifications().bodyOrThrow() }
            _state.update { s ->
                when (result) {
                    is Outcome.Ok -> s.copy(items = result.value.items, cursor = result.value.nextCursor, loading = false, error = null)
                    is Outcome.Failed -> s.copy(loading = false, error = result.message)
                }
            }
            refreshCount()
        }
    }

    fun more() {
        val s = _state.value
        val cursor = s.cursor ?: return
        if (s.loadingMore || s.loading) return
        _state.update { it.copy(loadingMore = true) }
        viewModelScope.launch {
            val result = call { api.notifications.listNotifications(cursor = cursor).bodyOrThrow() }
            _state.update { st ->
                when (result) {
                    is Outcome.Ok -> st.copy(
                        items = appendPage(st.items.orEmpty(), result.value.items),
                        cursor = result.value.nextCursor,
                        loadingMore = false,
                        error = null,
                    )
                    is Outcome.Failed -> st.copy(loadingMore = false, error = result.message)
                }
            }
        }
    }

    fun markAllRead() = markRead(null)

    /**
     * Tapping a notification: marks it read, closes the sheet and hands [go] where to go (a file
     * to open elsewhere once its details arrived; the sheet says why if they do not). False when
     * it is not in the list.
     */
    fun select(id: String, go: (NotificationTarget) -> Unit): Boolean {
        val item = _state.value.items.orEmpty().firstOrNull { it.id == id } ?: return false
        if (!item.read) markRead(listOf(id))
        when (val target = targetOf(item.subject)) {
            is NotificationTarget.File -> viewModelScope.launch {
                when (val result = call { api.files.getNode(target.id).bodyOrThrow().node }) {
                    is Outcome.Ok -> {
                        close()
                        go(NotificationTarget.FileNode(result.value))
                    }
                    is Outcome.Failed -> _state.update { it.copy(error = result.message) }
                }
            }
            else -> {
                close()
                go(target)
            }
        }
        return true
    }

    /** Shown read at once; the server's count follows. [ids] null: all of them. */
    private fun markRead(ids: List<String>?) {
        marks++
        val before = _state.value
        val newlyRead = before.items.orEmpty().count { !it.read && (ids == null || it.id in ids) }
        _state.update { s ->
            s.copy(
                items = s.items?.map { if (!it.read && (ids == null || it.id in ids)) it.copy(read = true) else it },
                unread = if (ids == null) 0 else (s.unread - newlyRead).coerceAtLeast(0),
                saving = true,
            )
        }
        viewModelScope.launch {
            val result = call { api.notifications.markRead(MarkReadRequest(ids)).bodyOrThrow() }
            _state.update { s ->
                when (result) {
                    is Outcome.Ok -> s.copy(unread = result.value.unread, saving = false)
                    is Outcome.Failed -> s.copy(saving = false, error = result.message)
                }
            }
        }
    }
}

/** The next page after the ones shown, without repeats. */
fun appendPage(shown: List<NotificationDto>, page: List<NotificationDto>): List<NotificationDto> {
    val seen = shown.map { it.id }.toSet()
    return shown + page.filter { it.id !in seen }
}
