package app.squirrel.lsdo.runtime

import io.ktor.client.plugins.defaultRequest
import io.ktor.client.request.get
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsText
import io.ktor.http.HttpStatusCode
import io.ktor.server.testing.testApplication
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals

class LsDoServerTest {
    @Test
    fun `ls then do over HTTP`() = testApplication {
        val registry = LsDoRegistry()
        registry.register("counter", Counter())
        application { lsdoRoutes(registry) }
        val client = createClient { defaultRequest { headers.append(LSDO_HEADER, "1") } }

        val ls = LsDoJson.parseToJsonElement(client.get("/ls").bodyAsText()).jsonObject
        assertEquals("counter", ls["screens"]?.jsonArray?.single()?.jsonObject?.get("id")?.jsonPrimitive?.content)

        val done = client.post("/do/counter") { setBody("""{"type":"add","n":2}""") }
        assertEquals(HttpStatusCode.OK, done.status)
        val state = LsDoJson.parseToJsonElement(done.bodyAsText()).jsonObject["screens"]?.jsonArray?.single()?.jsonObject?.get("state")
        assertEquals(2, state?.jsonObject?.get("count")?.jsonPrimitive?.int)
    }

    @Test
    fun `unknown screens are 404 and bodies that are not JSON are 400`() = testApplication {
        application { lsdoRoutes(LsDoRegistry()) }
        val client = createClient { defaultRequest { headers.append(LSDO_HEADER, "1") } }

        assertEquals(HttpStatusCode.NotFound, client.get("/ls/nope").status)
        assertEquals(HttpStatusCode.NotFound, client.post("/do/nope") { setBody("""{"type":"add"}""") }.status)
        assertEquals(HttpStatusCode.BadRequest, client.post("/do/nope") { setBody("not json") }.status)
    }

    @Test
    fun `requests without the LS-DO header are refused`() = testApplication {
        application { lsdoRoutes(LsDoRegistry()) }

        assertEquals(HttpStatusCode.Forbidden, client.get("/ls").status)
        assertEquals(HttpStatusCode.Forbidden, client.post("/do/counter") { setBody("""{"type":"add","n":1}""") }.status)
    }
}
