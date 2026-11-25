package app.squirrel.ui.friends

import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class FriendRow(
    @Doc("The account's username: what add_suggestion, remove and share_with take") val username: String,
    @Doc("Its display name") val name: String,
)

@Serializable
data class FriendsView(
    @Doc("Your friends, by name; null until the first load") val friends: List<FriendRow>? = null,
    @Doc("People you shared with or who shared with you, not friends yet; most recent first") val suggestions: List<FriendRow> = emptyList(),
    @Doc("The \"Add by username\" field") val username: String = "",
    val loading: Boolean = false,
    @Doc("An add or a removal is in flight") val busy: Boolean = false,
    @Doc("A load, a suggestion or a removal that failed") val error: String? = null,
    @Doc("Why the username typed could not be added, under the field") val addError: String? = null,
)

/** What an agent can do on the Friends screen (LS-DO); each maps to the call the screen makes. */
@Serializable
sealed interface FriendsIntent {
    @Serializable @SerialName("refresh") @Doc("Reload friends and suggestions; also tries again after an error")
    data object Refresh : FriendsIntent

    @Serializable @SerialName("add")
    @Doc("Type a username in \"Add by username\" and add it, as the Add button does. The account is not told. Refused (addError) for no such account, yourself, or 500 friends")
    data class Add(val username: String) : FriendsIntent

    @Serializable @SerialName("add_suggestion") @Doc("Add a suggestion by its username, in one tap")
    data class AddSuggestion(val username: String) : FriendsIntent

    @Serializable @SerialName("remove") @Doc("Take a friend off the list by username; nothing they were shared changes, and they are not told")
    data class Remove(val username: String) : FriendsIntent
}

internal fun friendsView(state: FriendsState) = FriendsView(
    friends = state.friends?.map { FriendRow(it.username, it.displayName) },
    suggestions = state.suggestions.map { FriendRow(it.username, it.displayName) },
    username = state.username,
    loading = state.loading,
    busy = state.busy,
    error = state.error,
    addError = state.addError,
)

/** The Friends screen as an LS-DO screen ("friends"). */
class FriendsLsDo(private val vm: FriendsViewModel) : LsDoScreen<FriendsView, FriendsIntent> {
    override val lsdo = lsdoSpec<FriendsView, FriendsIntent>()
    override val state: StateFlow<FriendsView> = vm.listing

    override suspend fun perform(intent: FriendsIntent): String? = when (intent) {
        FriendsIntent.Refresh -> null.also { vm.load() }
        is FriendsIntent.Add -> {
            vm.setUsername(intent.username)
            vm.addTyped()
        }
        is FriendsIntent.AddSuggestion ->
            if (vm.state.value.suggestions.none { it.username == intent.username }) "\"${intent.username}\" is not a suggestion."
            else vm.addSuggested(intent.username)
        is FriendsIntent.Remove ->
            if (vm.state.value.friends.orEmpty().none { it.username == intent.username }) "\"${intent.username}\" is not a friend."
            else vm.remove(intent.username)
    }

    override fun availableIntents(state: FriendsView): Set<String> = buildSet {
        addAll(listOf("refresh", "add"))
        if (state.suggestions.isNotEmpty()) add("add_suggestion")
        if (!state.friends.isNullOrEmpty()) add("remove")
    }

    override fun isBusy(state: FriendsView): Boolean = state.loading || state.busy
}
