package app.squirrel.ui.sharing

import app.squirrel.api.models.CreateShareRequest
import app.squirrel.api.models.ShareDto
import app.squirrel.api.models.SharePermission
import app.squirrel.shared.api.ApiError
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.api.okOrThrow
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

data class ShareState(
    /** Null until loaded. */
    val shares: List<ShareDto>? = null,
    val username: String = "",
    val permission: SharePermission = SharePermission.read,
    val error: String? = null,
    val busy: Boolean = false,
)

/**
 * The owner's Share sheet for one folder, file, album or photo: who it is shared with, sharing
 * with one more username, changing a permission, stopping.
 */
class ShareModel(private val api: SquirrelApi, private val subject: Subject, private val scope: CoroutineScope) {
    private val _state = MutableStateFlow(ShareState())
    val state: StateFlow<ShareState> = _state.asStateFlow()

    fun load() {
        scope.launch {
            when (val result = call { api.sharing.listShares(nodeId = subject.nodeId, albumId = subject.albumId).bodyOrThrow() }) {
                is Outcome.Ok -> _state.update { it.copy(shares = result.value, error = null) }
                is Outcome.Failed -> _state.update { it.copy(error = result.message) }
            }
        }
    }

    fun setUsername(value: String) = _state.update { it.copy(username = value) }

    fun setPermission(value: SharePermission) = _state.update { it.copy(permission = value) }

    /** Shares with the username typed; the field empties once it worked. */
    fun share() {
        val who = _state.value.username.trim()
        if (who.isEmpty()) return _state.update { it.copy(error = "Enter a username.") }
        _state.update { it.copy(busy = true, error = null) }
        scope.launch {
            val result = call { create(who, _state.value.permission) }
            _state.update { s ->
                when (result) {
                    is Outcome.Ok -> s.copy(busy = false, shares = upsertShare(s.shares.orEmpty(), result.value), username = "")
                    is Outcome.Failed -> s.copy(busy = false, error = result.message)
                }
            }
        }
    }

    fun change(share: ShareDto, permission: SharePermission) {
        if (share.permission == permission) return
        _state.update { it.copy(error = null) }
        scope.launch {
            when (val result = call { create(share.person.username, permission) }) {
                is Outcome.Ok -> _state.update { it.copy(shares = upsertShare(it.shares.orEmpty(), result.value)) }
                is Outcome.Failed -> _state.update { it.copy(error = result.message) }
            }
        }
    }

    fun stop(share: ShareDto) {
        _state.update { it.copy(error = null) }
        scope.launch {
            when (val result = call { api.sharing.deleteShare(share.id).okOrThrow() }) {
                is Outcome.Ok -> _state.update { s -> s.copy(shares = s.shares.orEmpty().filterNot { it.id == share.id }) }
                is Outcome.Failed -> _state.update { it.copy(error = result.message) }
            }
        }
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
