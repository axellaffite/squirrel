package app.squirrel.ui.notifications

import app.squirrel.api.models.NotificationKind
import app.squirrel.api.models.SubjectKind
import app.squirrel.shared.notifications.actorName
import app.squirrel.shared.notifications.notificationAction
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import io.github.axellaffite.lsdo.mapState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class NotificationRow(
    @Doc("What open_item takes") val id: String,
    @Doc("share or comment") val kind: String,
    @Doc("Who shared or commented") val actor: String,
    @Doc("The line after the actor's name, as the sheet shows it") val text: String,
    @Doc("When it happened (ISO 8601)") val createdAt: String,
    val read: Boolean,
    @Doc("folder, file or album") val subjectKind: String,
    val subjectId: String,
    @Doc("For a file: the folder open_item shows; null when the file opens on its own") val folderId: String? = null,
)

@Serializable
data class NotificationsView(
    @Doc("The bell's unread count") val unread: Long,
    @Doc("The notifications sheet is shown") val open: Boolean,
    @Doc("Newest first; null until the sheet first loaded") val items: List<NotificationRow>? = null,
    @Doc("More pages exist: more loads the next") val hasMore: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
)

/** What an agent can do with the bell and its sheet (LS-DO). */
@Serializable
sealed interface NotificationsIntent {
    @Serializable @SerialName("open") @Doc("Open the notifications sheet, as tapping the bell does (loads the list)")
    data object Open : NotificationsIntent

    @Serializable @SerialName("close") @Doc("Close the sheet")
    data object Close : NotificationsIntent

    @Serializable @SerialName("refresh") @Doc("Reload the count, and the list when the sheet is open")
    data object Refresh : NotificationsIntent

    @Serializable @SerialName("more") @Doc("Load the next page. Only when hasMore")
    data object More : NotificationsIntent

    @Serializable @SerialName("mark_all_read") @Doc("Mark every notification read")
    data object MarkAllRead : NotificationsIntent

    @Serializable @SerialName("open_item")
    @Doc("Tap a notification by its id: marks it read, closes the sheet and opens its subject in the Shared section (a file's folder, an album, a photo). Not a file shared on its own that is not a photo: it opens in another app")
    data class OpenItem(val id: String) : NotificationsIntent
}

internal fun notificationsView(state: NotificationsState) = NotificationsView(
    unread = state.unread,
    open = state.open,
    items = state.items?.map { item ->
        NotificationRow(
            id = item.id,
            kind = if (item.kind == NotificationKind.comment) "comment" else "share",
            actor = actorName(item),
            text = notificationAction(item),
            createdAt = item.createdAt.toString(),
            read = item.read,
            subjectKind = item.subject.kind.value,
            subjectId = item.subject.id,
            folderId = item.subject.folderId.takeIf { item.subject.kind == SubjectKind.file },
        )
    },
    hasMore = state.cursor != null,
    loading = state.loading || state.loadingMore,
    error = state.error,
)

/** The bell and its sheet as an LS-DO screen ("notifications"); [go] is the shell's own navigation. */
class NotificationsLsDo(
    private val vm: NotificationsViewModel,
    private val go: (NotificationTarget) -> Unit,
) : LsDoScreen<NotificationsView, NotificationsIntent> {
    override val lsdo = lsdoSpec<NotificationsView, NotificationsIntent>()
    override val state: StateFlow<NotificationsView> = vm.state.mapState(::notificationsView)

    override suspend fun perform(intent: NotificationsIntent): String? {
        when (intent) {
            NotificationsIntent.Open -> vm.open()
            NotificationsIntent.Close -> vm.close()
            NotificationsIntent.Refresh -> vm.resumed()
            NotificationsIntent.More -> if (vm.state.value.cursor == null) return "Every notification is shown." else vm.more()
            NotificationsIntent.MarkAllRead -> vm.markAllRead()
            is NotificationsIntent.OpenItem -> {
                val item = vm.state.value.items.orEmpty().firstOrNull { it.id == intent.id } ?: return "No notification \"${intent.id}\" in the list."
                if (targetOf(item.subject) is NotificationTarget.File) {
                    return "\"${item.subject.name}\" is a file: it opens in another app, which LS-DO does not drive."
                }
                vm.select(intent.id, go)
            }
        }
        return null
    }

    override fun availableIntents(state: NotificationsView): Set<String> = buildSet {
        add("refresh")
        add("mark_all_read")
        add(if (state.open) "close" else "open")
        if (state.hasMore) add("more")
        if (!state.items.isNullOrEmpty()) add("open_item")
    }

    override fun isBusy(state: NotificationsView): Boolean = state.loading || vm.state.value.saving
}
