package app.squirrel.ui.sharing

import app.squirrel.ui.testing.await
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.runReal
import app.squirrel.ui.testing.testApi
import app.squirrel.ui.testing.text
import io.ktor.client.engine.mock.respond
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpMethod
import io.ktor.http.HttpStatusCode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class CommentsModelTest {
    private fun comment(id: String, body: String = "c$id", mine: Boolean = false, canDelete: Boolean = false, author: Boolean = true) =
        """{"id":"$id","body":"$body","created_at":"2026-11-20T17:05:00Z","mine":$mine,"can_delete":$canDelete,""" +
            (if (author) """"author":{"username":"anna","display_name":"Anna"}""" else """"author":null""") + "}"

    private var pages = 0
    private val posted = mutableListOf<String>()

    private suspend fun model(scope: CoroutineScope, twoPages: Boolean = true) = CommentsModel(testApi { request ->
        val path = request.url.encodedPath
        when {
            path == "/api/comments" && request.method == HttpMethod.Get -> {
                pages += 1
                assertEquals("n1", request.url.parameters["node_id"])
                if (request.url.parameters["cursor"] == null) {
                    json("""{"items":[${comment("1", mine = true, canDelete = true)},${comment("2", author = false)}],"next_cursor":${if (twoPages) "\"c2\"" else "null"}}""")
                } else {
                    json("""{"items":[${comment("2", author = false)},${comment("3")}],"next_cursor":null}""")
                }
            }
            path == "/api/comments" -> {
                posted += request.text()
                json(comment("9", body = "new", mine = true, canDelete = true), HttpStatusCode.Created)
            }
            path == "/api/comments/1" && request.method == HttpMethod.Patch -> json(comment("1", body = "edited", mine = true, canDelete = true).replace("}", ""","edited_at":"2026-11-20T18:00:00Z"}""").replaceFirst(""","edited_at":"2026-11-20T18:00:00Z"}""", "}"))
            path == "/api/comments/1" && request.method == HttpMethod.Delete -> respond("", HttpStatusCode.NoContent)
            else -> respondError(HttpStatusCode.NotFound)
        }
    }, Subject.Node("n1"), scope)

    private fun scoped(block: suspend (CoroutineScope) -> Unit) = runReal {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        try { block(scope) } finally { scope.cancel() }
    }

    @Test
    fun showMoreBringsTheNextPageOnce() = scoped { scope ->
        val model = model(scope)
        model.load()
        val first = model.state.await { it.comments != null }
        assertEquals("c2", first.cursor)
        assertEquals("Deleted user", memberName(first.comments.orEmpty()[1].author))
        model.more()
        val all = model.state.await { it.comments?.size == 3 }
        assertEquals(listOf("1", "2", "3"), all.comments.orEmpty().map { it.id })
        assertNull(all.cursor)
    }

    @Test
    fun aPostedCommentShowsLastOnceEveryPageIsIn() = scoped { scope ->
        val model = model(scope, twoPages = false)
        model.load()
        model.state.await { it.comments != null }
        model.post()
        assertEquals("Write a comment first.", model.state.value.draftError)
        model.setDraft("  new ")
        model.post()
        val after = model.state.await { it.comments?.size == 3 }
        assertEquals("new", after.comments.orEmpty().last().body)
        assertEquals("", after.draft)
        assertTrue(posted.single().contains("\"body\":\"new\""))
        assertTrue(posted.single().contains("\"node_id\":\"n1\""))
    }

    @Test
    fun aPostedCommentWaitsForTheLastPage() = scoped { scope ->
        val model = model(scope)
        model.load()
        model.state.await { it.comments != null }
        model.setDraft("new")
        model.post()
        val after = model.state.await { !it.posting && it.draft.isEmpty() }
        assertEquals(listOf("1", "2"), after.comments.orEmpty().map { it.id })
    }

    @Test
    fun oneOwnCommentIsEditedAndAnotherDeleted() = scoped { scope ->
        val model = model(scope, twoPages = false)
        model.load()
        val comments = model.state.await { it.comments != null }.comments.orEmpty()
        model.startEdit(comments[1]) // not mine: ignored
        assertNull(model.state.value.editing)
        model.startEdit(comments[0])
        model.setEditDraft(" edited ")
        model.saveEdit()
        val edited = model.state.await { it.editing == null }.comments.orEmpty().first()
        assertEquals("edited", edited.body)
        assertTrue(edited.editedAt != null)

        model.askDelete(comments[1]) // cannot delete: ignored
        assertNull(model.state.value.deleting)
        model.askDelete(comments[0])
        model.confirmDelete()
        assertEquals(listOf("2"), model.state.await { it.comments?.size == 1 }.comments.orEmpty().map { it.id })
    }
}
