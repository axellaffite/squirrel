package app.squirrel.lsdo.runtime

import app.squirrel.lsdo.LsDoEntry
import app.squirrel.lsdo.lsdoSpec
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
    fun `ui reads the rendered tree as text or JSON and a do can carry it`() = testApplication {
        val registry = LsDoRegistry(capture = { listOf(UiNode(children = listOf(UiNode(role = "button", text = "Add", actions = listOf("click"))))) })
        registry.register("counter", Counter())
        application { lsdoRoutes(registry) }
        val client = createClient { defaultRequest { headers.append(LSDO_HEADER, "1") } }

        assertEquals("button \"Add\" [click]", client.get("/ui").bodyAsText())
        val json = LsDoJson.parseToJsonElement(client.get("/ui?format=json").bodyAsText()).jsonArray
        assertEquals("Add", json.single().jsonObject["text"]?.jsonPrimitive?.content)
        val done = LsDoJson.parseToJsonElement(client.post("/do/counter?ui=1") { setBody("""{"type":"add","n":1}""") }.bodyAsText()).jsonObject
        assertEquals("button \"Add\" [click]", done["ui"]?.jsonPrimitive?.content)
    }

    @Test
    fun `ui is 404 when the build cannot read it`() = testApplication {
        application { lsdoRoutes(LsDoRegistry()) }
        val client = createClient { defaultRequest { headers.append(LSDO_HEADER, "1") } }

        assertEquals(HttpStatusCode.NotFound, client.get("/ui").status)
    }

    @Test
    fun `the catalog lists screens not on show and one screen in full`() = testApplication {
        val registry = LsDoRegistry(catalog = listOf(LsDoEntry("counter", "A counter.", lsdoSpec<CounterState, CounterIntent>())))
        application { lsdoRoutes(registry) }
        val client = createClient { defaultRequest { headers.append(LSDO_HEADER, "1") } }

        val index = LsDoJson.parseToJsonElement(client.get("/catalog").bodyAsText()).jsonObject
        assertEquals("counter", index["screens"]?.jsonArray?.single()?.jsonObject?.get("name")?.jsonPrimitive?.content)
        assertEquals(HttpStatusCode.OK, client.get("/catalog/counter").status)
        assertEquals(HttpStatusCode.NotFound, client.get("/catalog/nope").status)
    }

    @Test
    fun `requests without the LS-DO header are refused`() = testApplication {
        application { lsdoRoutes(LsDoRegistry()) }

        assertEquals(HttpStatusCode.Forbidden, client.get("/ls").status)
        assertEquals(HttpStatusCode.Forbidden, client.post("/do/counter") { setBody("""{"type":"add","n":1}""") }.status)
    }
}
