package app.squirrel.ui.notifications

import app.squirrel.shared.api.SquirrelApi
import app.squirrel.ui.testing.json
import app.squirrel.ui.testing.node
import app.squirrel.ui.testing.testApi
import app.squirrel.ui.testing.text
import io.ktor.client.engine.mock.respondError
import io.ktor.http.HttpStatusCode

/** A fake inbox: two pages, mark read updates what the next calls return. */
class NotificationsServer {
    val marked = mutableListOf<String>()
    val read = mutableSetOf<String>()

    private fun item(id: String, kind: String, subject: String, at: String) =
        """{"id":"$id","kind":"$kind","actor":{"username":"anna","display_name":"Anna"},"subject":$subject,""" +
            """"permission":${if (kind == "share") "\"write\"" else "null"},"excerpt":${if (kind == "comment") "\"Nice\"" else "null"},""" +
            """"created_at":"$at","read":${id in read}}"""

    private val first = listOf(
        item("n1", "share", """{"kind":"folder","id":"trip","name":"Trip"}""", "2026-11-24T10:03:00Z"),
        item("n2", "comment", """{"kind":"file","id":"beach","name":"beach.jpg","mime":"image/jpeg","folder_id":"trip"}""", "2026-11-24T10:02:00Z"),
    )
    private val second = listOf(
        item("n3", "comment", """{"kind":"file","id":"tax","name":"tax.pdf","mime":"application/pdf"}""", "2026-11-24T10:01:00Z"),
        item("n4", "share", """{"kind":"album","id":"al","name":"Holidays"}""", "2026-11-24T10:00:00Z"),
    )
    private val all = listOf("n1", "n2", "n3", "n4")
    private val unread get() = all.count { it !in read }

    suspend fun api(): SquirrelApi = testApi { request ->
        when (request.url.encodedPath) {
            "/api/notifications/unread-count" -> json("""{"unread":$unread}""")
            "/api/notifications" -> if (request.url.parameters["cursor"] == "c2") {
                json("""{"items":[${second.joinToString(",")}],"next_cursor":null}""")
            } else {
                json("""{"items":[${first.joinToString(",")}],"next_cursor":"c2"}""")
            }
            "/api/nodes/tax" -> json("""{"node":${node("tax")},"access":"read","path":[]}""")
            "/api/notifications/read" -> {
                val body = request.text()
                marked += body
                val ids = Regex("\"(n\\d)\"").findAll(body).map { it.groupValues[1] }.toList()
                read += if ("ids" in body) ids else all
                json("""{"unread":$unread}""")
            }
            else -> respondError(HttpStatusCode.NotFound)
        }
    }
}
