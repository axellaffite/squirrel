package app.squirrel.ui.friends

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.FriendDto
import app.squirrel.api.models.MemberDto
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.RowGroup
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.TextField
import app.squirrel.ui.explore.SectionHead
import app.squirrel.ui.photos.ErrorRetry
import app.squirrel.ui.photos.RailClearance
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack

/** Friends, from the Shared tab: add by username or from suggestions, remove. */
@Composable
fun FriendsPage(vm: FriendsViewModel, onBack: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    LaunchedEffect(vm) { vm.load() }
    SystemBack(onBack = onBack)
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader("Friends", onBack = onBack, backLabel = "Back to shared")
        AddByUsername(state, vm)
        state.error?.let { ErrorRetry(it, vm::load) }
        val friends = state.friends
        if (friends == null) {
            if (state.error == null) Loading()
            return@Column
        }
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(Tokens.space2),
            contentPadding = PaddingValues(bottom = RailClearance),
        ) {
            if (friends.isEmpty()) {
                item(key = "empty") {
                    EmptyState(SquirrelIcon.Users, "No friends yet", "Add the people you share with, then pick them in one tap when you share.")
                }
            } else {
                item(key = "friends-head") { SectionHead("Friends", null) }
                item(key = "friends") { RowGroup { friends.forEach { friend -> FriendRow(friend, onRemove = { vm.drop(friend.username) }) } } }
            }
            if (state.suggestions.isNotEmpty()) {
                item(key = "suggestions-head") { SectionHead("Suggestions", null) }
                item(key = "suggestions") { RowGroup { state.suggestions.forEach { member -> SuggestionRow(member, onAdd = { vm.pick(member.username) }) } } }
            }
        }
    }
}

@Composable
private fun AddByUsername(state: FriendsState, vm: FriendsViewModel) {
    TextField(
        value = state.username,
        onValueChange = vm::setUsername,
        label = "Add by username",
        error = state.addError,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { vm.submit() }),
    )
    BasicText("They are not told, and can be removed any time.", style = SquirrelTheme.type.caption)
    Button("Add", onClick = vm::submit, enabled = !state.busy, full = true, icon = SquirrelIcon.UserPlus)
}

@Composable
private fun FriendRow(friend: FriendDto, onRemove: () -> Unit) {
    ListRow(
        name = friend.displayName,
        icon = SquirrelIcon.User,
        meta = "@${friend.username}",
        trailing = { IconButton(SquirrelIcon.X, label = "Remove ${friend.displayName} from friends", onClick = onRemove) },
    )
}

@Composable
private fun SuggestionRow(member: MemberDto, onAdd: () -> Unit) {
    ListRow(
        name = member.displayName,
        icon = SquirrelIcon.User,
        meta = "@${member.username}",
        trailing = { IconButton(SquirrelIcon.UserPlus, label = "Add ${member.displayName} to friends", onClick = onAdd) },
    )
}
