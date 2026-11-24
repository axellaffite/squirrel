package app.squirrel.lsdo.runtime

import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.server.application.Application
import io.ktor.server.cio.CIO
import io.ktor.server.engine.embeddedServer
import io.ktor.server.request.receiveText
import io.ktor.server.response.respondText
import io.ktor.server.routing.RoutingCall
import io.ktor.server.routing.get
import io.ktor.server.routing.post
import io.ktor.server.routing.routing
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** The port the app listens on, and the one `scripts/lsdo` forwards to. */
const val LSDO_PORT = 8765

/** Every request carries it: a web page on the phone cannot send it without a CORS preflight. */
const val LSDO_HEADER = "X-LsDo"

/**
 * The agent's door: `GET /ls`, `GET /ls/{screen}`, `POST /do/{screen}` with the intent as the
 * body. Loopback only: on a phone it is reached through `adb forward`, never over the network.
 */
fun startLsDoServer(registry: LsDoRegistry, port: Int = LSDO_PORT): LsDoServer {
    val server = embeddedServer(CIO, port = port, host = "127.0.0.1") { lsdoRoutes(registry) }.start(wait = false)
    return LsDoServer { server.stop() }
}

/** A running server; Ktor stays an implementation detail of the runtime. */
fun interface LsDoServer {
    fun stop()
}

fun Application.lsdoRoutes(registry: LsDoRegistry) {
    routing {
        get("/ls") { if (call.allowed()) call.json(registry.ls()) }
        get("/ls/{screen}") {
            if (!call.allowed()) return@get
            val id = call.parameters["screen"].orEmpty()
            val screen = registry.ls(id)
            if (screen == null) call.json(error("No screen \"$id\". Screens: ${registry.ids.joinToString().ifEmpty { "none" }}."), HttpStatusCode.NotFound)
            else call.json(screen)
        }
        post("/do/{screen}") {
            if (!call.allowed()) return@post
            val id = call.parameters["screen"].orEmpty()
            val body = try {
                LsDoJson.parseToJsonElement(call.receiveText())
            } catch (e: SerializationException) {
                return@post call.json(error("The body is not JSON: ${e.message}"), HttpStatusCode.BadRequest)
            }
            val outcome = registry.perform(id, body)
            call.json(outcome.toJson(), if (outcome.ok || id in registry.ids) HttpStatusCode.OK else HttpStatusCode.NotFound)
        }
    }
}

/** Refuses (403) a request without [LSDO_HEADER]; true when it may go on. */
private suspend fun RoutingCall.allowed(): Boolean {
    if (request.headers[LSDO_HEADER] != null) return true
    json(error("Missing the $LSDO_HEADER header."), HttpStatusCode.Forbidden)
    return false
}

private fun error(message: String): JsonObject = buildJsonObject {
    put("ok", false)
    put("message", message)
}

private suspend fun RoutingCall.json(body: JsonElement, status: HttpStatusCode = HttpStatusCode.OK) =
    respondText(LsDoJson.encodeToString(JsonElement.serializer(), body), ContentType.Application.Json, status)
