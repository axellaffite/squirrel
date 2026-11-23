package app.squirrel.ui.sharing

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.CommentDto
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.Card
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.Sheet
import app.squirrel.theme.components.TextField
import app.squirrel.theme.tabular
import kotlin.time.Clock

/**
 * The comment thread of a folder, file, album or photo, as a sheet ("Comments on “Holidays”").
 * [name] is the subject as a sentence reads it.
 */
@Composable
fun CommentsSheet(api: SquirrelApi, subject: Subject, name: String, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    val model = remember(subject) { CommentsModel(api, subject, scope) }
    LaunchedEffect(model) { model.load() }
    val state by model.state.collectAsStateWithLifecycle()
    val deleting = state.deleting
    if (deleting != null) {
        Sheet("Delete this comment?", onDismiss = model::cancelDelete) {
            BasicText("It is removed for everyone.", style = SquirrelTheme.type.body)
            Button("Delete comment", onClick = model::confirmDelete, variant = ButtonVariant.DangerSolid, full = true)
            Button("Cancel", onClick = model::cancelDelete, variant = ButtonVariant.Quiet, full = true)
        }
        return
    }
    Sheet("Comments on $name", onDismiss = onClose) {
        state.error?.let { Alert(it, tone = AlertTone.Error) }
        val comments = state.comments
        when {
            comments == null -> if (state.error == null) Loading()
            comments.isEmpty() -> BasicText("No comments yet.", style = SquirrelTheme.type.caption)
            else -> comments.forEach { comment -> CommentItem(comment, state, model) }
        }
        if (state.cursor != null) {
            Button("Show more comments", onClick = model::more, variant = ButtonVariant.Quiet, size = ButtonSize.Sm, enabled = !state.loadingMore)
        }
        TextField(
            value = state.draft,
            onValueChange = model::setDraft,
            label = "Add a comment",
            error = state.draftError,
            multiline = true,
            placeholder = "Write something…",
        )
        Button("Post comment", onClick = model::post, enabled = !state.posting, full = true)
    }
}

@Composable
private fun CommentItem(comment: CommentDto, state: CommentsState, model: CommentsModel) {
    val type = SquirrelTheme.type
    val author = memberName(comment.author)
    val whenText = commentWhen(comment.createdAt, Clock.System.now())
    Card {
        Column(verticalArrangement = Arrangement.spacedBy(Tokens.space1)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    BasicText(author, style = if (comment.author == null) type.label.copy(color = SquirrelTheme.scheme.inkMuted) else type.label)
                    BasicText(whenText + if (comment.editedAt != null) " · edited" else "", style = type.caption.tabular())
                }
                if (comment.mine && state.editing != comment.id) {
                    IconButton(SquirrelIcon.Pencil, label = "Edit your comment from $whenText", onClick = { model.startEdit(comment) })
                }
                if (comment.canDelete) IconButton(SquirrelIcon.Trash, label = "Delete the comment by $author", onClick = { model.askDelete(comment) })
            }
            if (state.editing == comment.id) {
                TextField(
                    value = state.editDraft,
                    onValueChange = model::setEditDraft,
                    label = "Edit your comment",
                    hideLabel = true,
                    error = state.editError,
                    multiline = true,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(Tokens.space2)) {
                    Button("Cancel", onClick = model::cancelEdit, variant = ButtonVariant.Quiet, size = ButtonSize.Sm)
                    Button("Save", onClick = model::saveEdit, size = ButtonSize.Sm)
                }
            } else {
                BasicText(comment.body, style = type.body)
            }
        }
    }
}
