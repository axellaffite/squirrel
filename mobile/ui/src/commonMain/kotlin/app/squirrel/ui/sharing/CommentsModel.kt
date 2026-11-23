package app.squirrel.ui.sharing

import app.squirrel.api.models.CommentDto
import app.squirrel.api.models.CreateCommentRequest
import app.squirrel.api.models.EditCommentRequest
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.shared.api.bodyOrThrow
import app.squirrel.shared.api.okOrThrow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Comments fetched per page. */
const val COMMENT_PAGE = 50

data class CommentsState(
    /** Oldest first; null until the first page arrived. */
    val comments: List<CommentDto>? = null,
    /** More (newer) comments to fetch: "Show more comments". */
    val cursor: String? = null,
    val loadingMore: Boolean = false,
    val error: String? = null,
    val draft: String = "",
    val draftError: String? = null,
    val posting: Boolean = false,
    /** The comment being edited, and its text. */
    val editing: String? = null,
    val editDraft: String = "",
    val editError: String? = null,
    /** The comment waiting for "Delete this comment?". */
    val deleting: CommentDto? = null,
)

/** One flat comment thread: read, post, edit one's own, delete when allowed. */
class CommentsModel(private val api: SquirrelApi, private val subject: Subject, private val scope: CoroutineScope) {
    private val _state = MutableStateFlow(CommentsState())
    val state: StateFlow<CommentsState> = _state.asStateFlow()

    fun load() = page(null)

    fun more() {
        val s = _state.value
        if (s.cursor == null || s.loadingMore) return
        _state.update { it.copy(loadingMore = true) }
        page(s.cursor)
    }

    private fun page(after: String?) {
        scope.launch {
            val result = call {
                api.comments.listComments(nodeId = subject.nodeId, albumId = subject.albumId, cursor = after, limit = COMMENT_PAGE).bodyOrThrow()
            }
            _state.update { s ->
                when (result) {
                    is Outcome.Ok -> s.copy(
                        comments = mergeComments(s.comments.orEmpty(), result.value.items),
                        cursor = result.value.nextCursor,
                        error = null,
                        loadingMore = false,
                    )
                    is Outcome.Failed -> s.copy(error = result.message, loadingMore = false)
                }
            }
        }
    }

    fun setDraft(value: String) = _state.update { it.copy(draft = value, draftError = null) }

    fun post() {
        val s = _state.value
        if (s.posting) return
        commentProblem(s.draft)?.let { problem -> return _state.update { it.copy(draftError = problem) } }
        _state.update { it.copy(posting = true, draftError = null) }
        scope.launch {
            val body = s.draft.trim()
            val result = call {
                api.comments.createComment(CreateCommentRequest(body = body, albumId = subject.albumId, nodeId = subject.nodeId)).bodyOrThrow()
            }
            _state.update { now ->
                when (result) {
                    // Oldest first: a new comment is last once every page is in; otherwise the next page brings it.
                    is Outcome.Ok -> now.copy(
                        posting = false,
                        draft = "",
                        comments = if (now.cursor == null) now.comments.orEmpty() + result.value else now.comments,
                    )
                    is Outcome.Failed -> now.copy(posting = false, draftError = result.message)
                }
            }
        }
    }

    fun startEdit(comment: CommentDto) {
        if (!comment.mine) return
        _state.update { it.copy(editing = comment.id, editDraft = comment.body, editError = null) }
    }

    fun setEditDraft(value: String) = _state.update { it.copy(editDraft = value, editError = null) }

    fun cancelEdit() = _state.update { it.copy(editing = null, editError = null) }

    fun saveEdit() {
        val s = _state.value
        val id = s.editing ?: return
        commentProblem(s.editDraft)?.let { problem -> return _state.update { it.copy(editError = problem) } }
        scope.launch {
            val result = call { api.comments.editComment(id, EditCommentRequest(s.editDraft.trim())).bodyOrThrow() }
            _state.update { now ->
                when (result) {
                    is Outcome.Ok -> now.copy(editing = null, comments = now.comments.orEmpty().map { if (it.id == id) result.value else it })
                    is Outcome.Failed -> now.copy(editError = result.message)
                }
            }
        }
    }

    fun askDelete(comment: CommentDto) {
        if (comment.canDelete) _state.update { it.copy(deleting = comment) }
    }

    fun cancelDelete() = _state.update { it.copy(deleting = null) }

    fun confirmDelete() {
        val comment = _state.value.deleting ?: return
        scope.launch {
            val result = call { api.comments.deleteComment(comment.id).okOrThrow() }
            _state.update { now ->
                when (result) {
                    is Outcome.Ok -> now.copy(deleting = null, comments = now.comments.orEmpty().filterNot { it.id == comment.id })
                    is Outcome.Failed -> now.copy(deleting = null, error = result.message)
                }
            }
        }
    }
}
