package app.squirrel.ui.friends

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.squirrel.api.models.AddFriendRequest
import app.squirrel.api.models.FriendDto
import app.squirrel.api.models.MemberDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.api.okOrThrow
import io.github.axellaffite.lsdo.mapState
import app.squirrel.ui.sharing.Outcome
import app.squirrel.ui.sharing.call
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

private const val NOT_FOUND = 404

data class FriendsState(
    /** Null until loaded; by display name. */
    val friends: List<FriendDto>? = null,
    /** People the caller shared with or got a share from, not friends yet; most recent first. */
    val suggestions: List<MemberDto> = emptyList(),
    /** The "Add by username" field. */
    val username: String = "",
    val loading: Boolean = false,
    /** A load, a suggestion or a removal that failed. */
    val error: String? = null,
    /** Why the username typed could not be added (the server's message). */
    val addError: String? = null,
    /** An add or a removal is in flight. */
    val busy: Boolean = false,
)

/**
 * The Friends screen: a one-way list of accounts to share with in one tap. Adding needs no
 * consent and tells no one; neither does removing.
 */
class FriendsViewModel(private val api: SquirrelApi) : ViewModel() {
    private val _state = MutableStateFlow(FriendsState())
    val state: StateFlow<FriendsState> = _state.asStateFlow()

    /** The screen as LS-DO shows it (see [FriendsLsDo]). */
    val listing: StateFlow<FriendsView> = state.mapState(::friendsView)

    init {
        load()
    }

    fun load() {
        if (_state.value.loading) return
        _state.update { it.copy(loading = true) }
        viewModelScope.launch { fetch() }
    }

    /** Friends and suggestions from the server; [FriendsState.loading] ends with it. */
    private suspend fun fetch() {
        _state.update { it.copy(loading = true) }
        val result = call { api.friends.listFriends().bodyOrThrow() }
        _state.update { s ->
            when (result) {
                is Outcome.Ok -> s.copy(friends = result.value.friends, suggestions = result.value.suggestions, loading = false, error = null)
                is Outcome.Failed -> s.copy(loading = false, error = result.message)
            }
        }
    }

    fun setUsername(value: String) = _state.update { it.copy(username = value, addError = null) }

    fun submit() {
        viewModelScope.launch { addTyped() }
    }

    fun pick(username: String) {
        viewModelScope.launch { addSuggested(username) }
    }

    fun drop(username: String) {
        viewModelScope.launch { remove(username) }
    }

    /** Adds the username typed; the field empties once it worked. The refusal, or null. */
    suspend fun addTyped(): String? {
        val who = _state.value.username.trim()
        if (who.isEmpty()) return "Enter a username.".also { message -> _state.update { it.copy(addError = message) } }
        _state.update { it.copy(busy = true, addError = null) }
        val result = add(who)
        _state.update { s ->
            when (result) {
                is Outcome.Ok -> added(s, result.value).copy(username = "")
                is Outcome.Failed -> s.copy(busy = false, addError = result.message)
            }
        }
        return (result as? Outcome.Failed)?.message
    }

    /** Adds a suggestion in one tap, leaving the field as it is. The refusal, or null. */
    suspend fun addSuggested(username: String): String? {
        _state.update { it.copy(busy = true, error = null) }
        val result = add(username)
        _state.update { s ->
            when (result) {
                is Outcome.Ok -> added(s, result.value)
                is Outcome.Failed -> s.copy(busy = false, error = result.message)
            }
        }
        return (result as? Outcome.Failed)?.message
    }

    /**
     * Takes [username] off the list; one the server no longer has goes quietly. Then reloads, still
     * busy so no add or removal races it: someone you share with is back in suggestions at once.
     * The refusal, or null.
     */
    suspend fun remove(username: String): String? {
        _state.update { it.copy(busy = true, error = null) }
        val result = call { api.friends.removeFriend(username).okOrThrow() }
        val failed = (result as? Outcome.Failed)?.takeIf { it.status != NOT_FOUND }
        if (failed != null) {
            _state.update { it.copy(busy = false, error = failed.message) }
            return failed.message
        }
        _state.update { s -> s.copy(friends = s.friends?.filterNot { it.username == username }) }
        fetch()
        _state.update { it.copy(busy = false) }
        return null
    }

    private suspend fun add(username: String): Outcome<FriendDto> =
        call { api.friends.addFriend(AddFriendRequest(username = username)).bodyOrThrow() }

    private fun added(state: FriendsState, friend: FriendDto) = state.copy(
        busy = false,
        friends = withFriend(state.friends.orEmpty(), friend),
        suggestions = state.suggestions.filterNot { it.username == friend.username },
    )
}
