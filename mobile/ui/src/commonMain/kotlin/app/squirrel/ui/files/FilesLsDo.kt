package app.squirrel.ui.files

import app.squirrel.api.models.NodeDto
import app.squirrel.lsdo.Doc
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** What an agent can do on a folder page (LS-DO); each maps to the [FilesViewModel] call the UI makes. */
@Serializable
sealed interface FilesIntent {
    @Serializable @SerialName("enter") @Doc("Open a folder by id; no id: where this browser started")
    data class Enter(val id: String? = null) : FilesIntent

    @Serializable @SerialName("up") @Doc("One folder up")
    data object Up : FilesIntent

    @Serializable @SerialName("top") @Doc("Back to where this browser starts: the top level of Files (in Shared: the shared folder)")
    data object Top : FilesIntent

    @Serializable @SerialName("more") @Doc("Load the next page of rows")
    data object More : FilesIntent

    @Serializable @SerialName("refresh") @Doc("Reload the folder from its first page")
    data object Refresh : FilesIntent

    @Serializable @SerialName("retry") @Doc("Try the failed load again")
    data object Retry : FilesIntent

    @Serializable @SerialName("open") @Doc("Open a row: a folder is entered, a file opens in another app")
    data class Open(val id: String, @Doc("Skip the large-download question") val confirmed: Boolean = false) : FilesIntent

    @Serializable @SerialName("dismiss_confirm") @Doc("Answer no to the large-download question")
    data object DismissConfirm : FilesIntent

    @Serializable @SerialName("create_folder") @Doc("New folder in the folder shown")
    data class CreateFolder(val name: String) : FilesIntent

    @Serializable @SerialName("rename") @Doc("Rename a row of the folder shown")
    data class Rename(val id: String, val name: String) : FilesIntent

    @Serializable @SerialName("move") @Doc("Move a row of the folder shown into another folder")
    data class Move(val id: String, val parentId: String) : FilesIntent
}

/** Runs [intent] through the same calls as the screen; a refusal message, or null. */
internal suspend fun FilesViewModel.performIntent(intent: FilesIntent): String? = when (intent) {
    is FilesIntent.Enter -> done { enter(intent.id ?: root) }
    FilesIntent.Up -> if (up()) null else "Already at the top of this browser."
    FilesIntent.Top -> if (atRoot) "Already at the top of this browser." else done { enter(root) }
    FilesIntent.More -> done { more() }
    FilesIntent.Refresh -> done { refresh() }
    FilesIntent.Retry -> done { retry() }
    is FilesIntent.Open -> withRow(intent.id) { done { open(it, intent.confirmed) } }
    FilesIntent.DismissConfirm -> done { dismissConfirm() }
    is FilesIntent.CreateFolder -> createFolder(intent.name)
    is FilesIntent.Rename -> withRow(intent.id) { rename(it, intent.name) }
    is FilesIntent.Move -> withRow(intent.id) { move(it, intent.parentId) }
}

/** The actions that make sense now: no "up" at the root, no edits without the right to edit. */
internal fun FilesViewModel.intentsFor(state: FilesState): Set<String> = buildSet {
    addAll(listOf("enter", "more", "refresh", "retry", "open"))
    if (!atRoot) addAll(listOf("up", "top"))
    if (state.confirm != null) add("dismiss_confirm")
    if (state.canAdd) add("create_folder")
    if (state.can.edit) addAll(listOf("rename", "move"))
}

/** A call the screen cannot refuse. */
private inline fun done(call: () -> Unit): String? {
    call()
    return null
}

/** Runs [action] on row [id] of the folder shown, or says it is not there. */
private inline fun FilesViewModel.withRow(id: String, action: (NodeDto) -> String?): String? {
    val node = state.value.items.firstOrNull { it.id == id } ?: return "No row \"$id\" in the folder shown."
    return action(node)
}
