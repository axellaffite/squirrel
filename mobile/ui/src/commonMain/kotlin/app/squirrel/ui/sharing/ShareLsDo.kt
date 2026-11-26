package app.squirrel.ui.sharing

import app.squirrel.api.models.ShareDto
import app.squirrel.api.models.SharePermission
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import app.squirrel.ui.friends.FriendRow
import io.github.axellaffite.lsdo.mapState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class ShareRow(
    @Doc("Who has access: what change and stop take") val username: String,
    val name: String,
    @Doc("May edit (\"Write\"); else view only (\"Read\")") val canEdit: Boolean,
)

@Serializable
data class ShareView(
    @Doc("The folder, file, album or photo being shared") val name: String,
    @Doc("Who has access; null until loaded") val shares: List<ShareRow>? = null,
    @Doc("Friends without access yet, shown as one-tap picks: share_with takes their username") val picks: List<FriendRow> = emptyList(),
    @Doc("The Username field") val username: String = "",
    @Doc("The permission chosen for the next share (Write); else Read") val canEdit: Boolean = false,
    val error: String? = null,
    @Doc("A share is in flight") val busy: Boolean = false,
    @Doc("Shares are view only (a dynamic album): no set_permission, no change") val readOnly: Boolean = false,
)

/** What an agent can do in the Share sheet (LS-DO); each maps to the call the sheet makes. */
@Serializable
sealed interface ShareIntent {
    @Serializable @SerialName("set_permission") @Doc("Choose Read or Write for the next share (the sheet's Read/Write control)")
    data class SetPermission(val canEdit: Boolean) : ShareIntent

    @Serializable @SerialName("share") @Doc("Type a username and tap Share, with the permission chosen; the field empties once it worked")
    data class Share(val username: String) : ShareIntent

    @Serializable @SerialName("share_with") @Doc("Tap a friend's pick (one in picks): shares at once with the permission chosen")
    data class ShareWith(val username: String) : ShareIntent

    @Serializable @SerialName("change") @Doc("Change the permission of someone in shares")
    data class Change(val username: String, val canEdit: Boolean) : ShareIntent

    @Serializable @SerialName("stop") @Doc("Stop sharing with someone in shares")
    data class Stop(val username: String) : ShareIntent

    @Serializable @SerialName("close") @Doc("Close the sheet")
    data object Close : ShareIntent
}

private fun permissionOf(canEdit: Boolean) = if (canEdit) SharePermission.write else SharePermission.read

internal fun shareView(name: String, state: ShareState, readOnly: Boolean = false) = ShareView(
    name = name,
    shares = state.shares?.map { ShareRow(it.person.username, it.person.displayName, it.permission == SharePermission.write) },
    picks = state.picks.map { FriendRow(it.username, it.displayName) },
    username = state.username,
    canEdit = state.permission == SharePermission.write,
    error = state.error,
    busy = state.busy,
    readOnly = readOnly,
)

/** The Share sheet as an LS-DO screen ("share"), while it is open. */
class ShareLsDo(
    private val model: ShareModel,
    name: String,
    private val onClose: () -> Unit,
) : LsDoScreen<ShareView, ShareIntent> {
    override val lsdo = lsdoSpec<ShareView, ShareIntent>()
    override val state: StateFlow<ShareView> = model.state.mapState { shareView(name, it, model.readOnly) }

    override suspend fun perform(intent: ShareIntent): String? = when (intent) {
        is ShareIntent.SetPermission ->
            if (model.readOnly && intent.canEdit) READ_ONLY_SHARES else null.also { model.setPermission(permissionOf(intent.canEdit)) }
        is ShareIntent.Share -> {
            model.setUsername(intent.username)
            model.shareTyped()
        }
        is ShareIntent.ShareWith -> when {
            model.state.value.picks.any { it.username == intent.username } -> model.shareWith(intent.username)
            model.state.value.shares.orEmpty().any { it.person.username == intent.username } -> "\"${intent.username}\" already has access."
            else -> "\"${intent.username}\" is not a friend."
        }
        is ShareIntent.Change -> withShare(intent.username) { model.changeTo(it, permissionOf(intent.canEdit)) }
        is ShareIntent.Stop -> withShare(intent.username) { model.stopSharing(it) }
        ShareIntent.Close -> null.also { onClose() }
    }

    override fun availableIntents(state: ShareView): Set<String> = buildSet {
        addAll(listOf("share", "close"))
        if (!state.readOnly) add("set_permission")
        if (state.picks.isNotEmpty()) add("share_with")
        if (!state.shares.isNullOrEmpty()) {
            add("stop")
            if (!state.readOnly) add("change")
        }
    }

    override fun isBusy(state: ShareView): Boolean = state.busy || (state.shares == null && state.error == null)

    private suspend fun withShare(username: String, action: suspend (ShareDto) -> String?): String? {
        val share = model.state.value.shares.orEmpty().firstOrNull { it.person.username == username } ?: return "Not shared with \"$username\"."
        return action(share)
    }
}
