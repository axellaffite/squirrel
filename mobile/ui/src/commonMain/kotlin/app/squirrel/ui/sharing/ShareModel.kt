package app.squirrel.ui.sharing

import app.squirrel.api.models.CreateShareRequest
import app.squirrel.api.models.FriendDto
import app.squirrel.api.models.ShareDto
import app.squirrel.api.models.SharePermission
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.api.okOrThrow
import app.squirrel.ui.friends.friendPicks
import app.squirrel.ui.photos.UNREACHABLE
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

private const val ENTER_USERNAME = "Enter a username."
const val READ_ONLY_SHARES = "Dynamic albums are shared read-only."

data class ShareState(
    /** Null until loaded. */
    val shares: List<ShareDto>? = null,
    /** The caller's friends; empty when they could not be loaded (the sheet works without them). */
    val friends: List<FriendDto> = emptyList(),
    val username: String = "",
    val permission: SharePermission = SharePermission.read,
    val error: String? = null,
    val busy: Boolean = false,
) {
    /** Friends without access yet: one tap shares with them. */
    val picks: List<FriendDto> get() = friendPicks(friends, shares)
}

/**
 * The owner's Share sheet for one folder, file, album or photo: who it is shared with, sharing
 * with one more username or a friend in one tap, changing a permission, stopping. [readOnly]
 * (a dynamic album): shares are for viewing only.
 */
class ShareModel(
    private val api: SquirrelApi,
    private val subject: Subject,
    private val scope: CoroutineScope,
    val readOnly: Boolean = false,
) {
    private val _state = MutableStateFlow(ShareState())
    val state: StateFlow<ShareState> = _state.asStateFlow()

    fun load() {
        scope.launch {
            when (val result = call { api.sharing.listShares(nodeId = subject.nodeId, albumId = subject.albumId).bodyOrThrow() }) {
                is Outcome.Ok -> _state.update { it.copy(shares = result.value, error = null) }
                is Outcome.Failed -> _state.update { it.copy(error = result.message) }
            }
        }
        scope.launch {
            // Picks are a shortcut: without them (an older server, a failure) typing still works.
            val result = call { api.friends.listFriends().bodyOrThrow() }
            if (result is Outcome.Ok) _state.update { it.copy(friends = result.value.friends) }
        }
    }

    fun setUsername(value: String) = _state.update { it.copy(username = value) }

    fun setPermission(value: SharePermission) {
        if (readOnly && value != SharePermission.read) return
        _state.update { it.copy(permission = value) }
    }

    /** Shares with the username typed; the field empties once it worked. */
    fun share() {
        val who = typed() ?: return
        scope.launch { add(who, clearField = true) }
    }

    /** A friend's pick: shares at once with the permission chosen. */
    fun pick(friend: FriendDto) {
        scope.launch { shareWith(friend.username) }
    }

    fun change(share: ShareDto, permission: SharePermission) {
        scope.launch { changeTo(share, permission) }
    }

    fun stop(share: ShareDto) {
        scope.launch { stopSharing(share) }
    }

    /** [share], awaited: the refusal, or null. */
    suspend fun shareTyped(): String? {
        val who = typed() ?: return ENTER_USERNAME
        return add(who, clearField = true)
    }

    /** The username typed, trimmed; null (and said) when there is none. */
    private fun typed(): String? = _state.value.username.trim().ifEmpty {
        _state.update { it.copy(error = ENTER_USERNAME) }
        null
    }

    /** [pick], awaited: the typed username stays as it is. The refusal, or null. */
    suspend fun shareWith(username: String): String? = add(username, clearField = false)

    /** [change], awaited: the refusal, or null. */
    suspend fun changeTo(share: ShareDto, permission: SharePermission): String? {
        if (share.permission == permission) return null
        if (readOnly && permission != SharePermission.read) return READ_ONLY_SHARES.also { message -> _state.update { it.copy(error = message) } }
        _state.update { it.copy(error = null) }
        return when (val result = call { create(share.person.username, permission) }) {
            is Outcome.Ok -> null.also { _state.update { it.copy(shares = upsertShare(it.shares.orEmpty(), result.value)) } }
            is Outcome.Failed -> result.message.also { message -> _state.update { it.copy(error = message) } }
        }
    }

    /** [stop], awaited: the refusal, or null. */
    suspend fun stopSharing(share: ShareDto): String? {
        _state.update { it.copy(error = null) }
        return when (val result = call { api.sharing.deleteShare(share.id).okOrThrow() }) {
            is Outcome.Ok -> null.also { _state.update { s -> s.copy(shares = s.shares.orEmpty().filterNot { it.id == share.id }) } }
            is Outcome.Failed -> result.message.also { message -> _state.update { it.copy(error = message) } }
        }
    }

    private suspend fun add(username: String, clearField: Boolean): String? {
        _state.update { it.copy(busy = true, error = null) }
        val result = call { create(username, if (readOnly) SharePermission.read else _state.value.permission) }
        _state.update { s ->
            when (result) {
                is Outcome.Ok -> s.copy(busy = false, shares = upsertShare(s.shares.orEmpty(), result.value), username = if (clearField) "" else s.username)
                is Outcome.Failed -> s.copy(busy = false, error = result.message)
            }
        }
        return (result as? Outcome.Failed)?.message
    }

    private suspend fun create(username: String, permission: SharePermission): ShareDto =
        api.sharing.createShare(CreateShareRequest(permission = permission, username = username, albumId = subject.albumId, nodeId = subject.nodeId)).bodyOrThrow()
}

/** A call's value, or what to tell the user. */
sealed interface Outcome<out T> {
    data class Ok<T>(val value: T) : Outcome<T>
    data class Failed(val message: String, val status: Int = 0) : Outcome<Nothing>
}

/**
 * Runs a call for a sheet: the server's own message when it refused (safe to show), the usual
 * "could not be reached" otherwise. A stray cancellation is a failure, as in `attempt`.
 */
suspend fun <T> call(block: suspend () -> T): Outcome<T> = try {
    Outcome.Ok(block())
} catch (e: CancellationException) {
    if (!currentCoroutineContext().isActive) throw e
    Outcome.Failed(UNREACHABLE)
} catch (e: ApiError) {
    Outcome.Failed(if (e.fromServer) e.message ?: UNREACHABLE else UNREACHABLE, e.status)
} catch (_: Exception) {
    Outcome.Failed(UNREACHABLE)
}
