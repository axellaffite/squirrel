package app.squirrel.ui.send

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.api.models.FriendDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.ListRow
import app.squirrel.theme.components.RowGroup
import app.squirrel.theme.components.Progress
import app.squirrel.theme.components.TextField
import app.squirrel.theme.components.ground
import app.squirrel.ui.AppGraph
import app.squirrel.ui.AppState
import app.squirrel.ui.files.FolderPickerSheet
import app.squirrel.ui.files.PickedFile
import app.squirrel.ui.images.LocalImageLoader
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import app.squirrel.ui.signin.SignInFlow

/** Files named on the page before "and N more". */
private const val NAMES_SHOWN = 5

/**
 * The root of "Send with Squirrel", the system share target (Android's share sheet, iOS's Share
 * extension): sign in first when needed, then [SendPage]. [onClose] ends the share (sent or not).
 */
@Composable
fun SendApp(graph: AppGraph, files: List<PickedFile>, onClose: () -> Unit) {
    SquirrelTheme {
        val state by graph.state.collectAsStateWithLifecycle()
        val images by graph.imageLoader.collectAsStateWithLifecycle()
        CompositionLocalProvider(LocalImageLoader provides images) {
            Box(Modifier.fillMaxSize().ground()) {
                when (val current = state) {
                    AppState.Loading -> Unit
                    is AppState.NeedsSignIn -> SignInFlow(graph, current.attempt, current.serverUrl)
                    is AppState.SignedIn -> graph.api?.let { api -> SendPage(api, files, onClose) }
                }
            }
        }
    }
}

/** Holds the send with the screen (not the session): leaving the share ends it. */
private class SendViewModel(api: SquirrelApi, files: List<PickedFile>) : ViewModel() {
    val model = SendModel(api, files, viewModelScope)
}

@Composable
private fun SendPage(api: SquirrelApi, files: List<PickedFile>, onClose: () -> Unit) {
    val model = viewModel { SendViewModel(api, files) }.model
    LaunchedEffect(model) { model.load() }
    val state by model.state.collectAsStateWithLifecycle()
    var choosing by remember { mutableStateOf(false) }
    val sending = state.phase == SendPhase.Sending
    SystemBack(enabled = !sending, onBack = onClose)
    Column(
        Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).padding(horizontal = Tokens.gutter).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(Tokens.space3),
    ) {
        PageHeader("Send with Squirrel", onBack = if (sending) null else onClose, backLabel = "Cancel")
        FileNames(state.names)
        if (state.names.isEmpty()) {
            Alert("Nothing to send: Squirrel sends files, and none came with this share.", tone = AlertTone.Error)
            Button("Close", onClick = onClose, full = true)
            return@Column
        }
        if (state.phase == SendPhase.Sent) {
            Alert(state.notice.orEmpty(), tone = AlertTone.Success)
            Button("Done", onClick = onClose, full = true)
            return@Column
        }
        Recipient(state, enabled = !sending, onPick = model::pick, onType = model::type, onSend = model::send)
        BasicText("Into", style = SquirrelTheme.type.caption)
        RowGroup {
            ListRow(
                state.folderLabel,
                icon = SquirrelIcon.Folder,
                meta = if (state.folder == null) "Created when missing" else null,
                onClick = if (sending) null else ({ choosing = true }),
                trailing = { BasicText("Change", style = SquirrelTheme.type.label) },
            )
        }
        if (state.folder != null && !sending) {
            Button("Use ${SENT_ROOT} › ${SENT_FOLDER}", onClick = { model.choose(null) }, variant = ButtonVariant.Quiet, size = ButtonSize.Sm)
        }
        state.error?.let { Alert(it, tone = AlertTone.Error) }
        if (sending) {
            Progress(state.progress, label = "Sending to ${state.recipient}")
        } else {
            Button("Send", onClick = model::send, enabled = state.canSend, full = true, icon = SquirrelIcon.Share)
        }
    }
    if (choosing) {
        FolderPickerSheet(api, "Send into", "Send here", onPick = { trail -> null.also { model.choose(trail) } }, onClose = { choosing = false })
    }
}

/** The files handed over: the first few by name, then how many more. */
@Composable
private fun FileNames(names: List<String>) {
    if (names.isNotEmpty()) RowGroup { names.take(NAMES_SHOWN).forEach { ListRow(it, icon = SquirrelIcon.File) } }
    val more = names.size - NAMES_SHOWN
    if (more > 0) BasicText("and $more more", style = SquirrelTheme.type.caption)
}

/** Who gets the files: a friend in one tap, or a username typed. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Recipient(state: SendState, enabled: Boolean, onPick: (String) -> Unit, onType: (String) -> Unit, onSend: () -> Unit) {
    val friends = state.friends.orEmpty()
    BasicText("To", style = SquirrelTheme.type.caption)
    if (friends.isNotEmpty()) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(Tokens.space2), verticalArrangement = Arrangement.spacedBy(Tokens.space1)) {
            friends.forEach { friend -> FriendChip(friend, picked = friend.username == state.picked, enabled = enabled) { onPick(friend.username) } }
        }
    }
    TextField(
        value = state.typed,
        onValueChange = onType,
        label = if (friends.isEmpty()) "Username" else "Or a username",
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Send),
        keyboardActions = KeyboardActions(onSend = { onSend() }),
    )
    BasicText(
        state.recipient?.let { "Shared with @$it, read only." } ?: "Someone with an account on this Squirrel.",
        style = SquirrelTheme.type.caption,
        modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
    )
}

@Composable
private fun FriendChip(friend: FriendDto, picked: Boolean, enabled: Boolean, onClick: () -> Unit) {
    Button(
        friend.displayName,
        onClick = onClick,
        modifier = Modifier.semantics {
            contentDescription = "Send to ${friend.displayName}"
            selected = picked
        },
        variant = if (picked) ButtonVariant.Primary else ButtonVariant.Secondary,
        size = ButtonSize.Sm,
        enabled = enabled,
        icon = if (picked) SquirrelIcon.Check else SquirrelIcon.User,
    )
}
