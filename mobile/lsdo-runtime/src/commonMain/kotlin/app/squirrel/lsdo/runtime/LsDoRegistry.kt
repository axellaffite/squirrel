package app.squirrel.lsdo.runtime

import app.squirrel.lsdo.LsDo
import app.squirrel.lsdo.LsDoScreen
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

/** The JSON of states and intents: defaults written out, intents chosen by their "type". */
val LsDoJson: Json = Json {
    encodeDefaults = true
    classDiscriminator = "type"
}

/**
 * What a do answers: whether it ran, the screen's refusal or the error, the screens after it,
 * and what is rendered then ([ui], when asked for: the compact text of [render]).
 */
data class LsDoOutcome(val ok: Boolean, val message: String?, val screens: JsonObject, val ui: String? = null) {
    fun toJson(): JsonObject = buildJsonObject {
        put("ok", ok)
        put("message", message)
        put("screens", screens["screens"] ?: JsonArray(emptyList()))
        ui?.let { put("ui", it) }
    }
}

/**
 * The screens on show, keyed by a unique id ("files", then "files-2" for a second one). Screens
 * register from the UI thread while the server reads from its own: the list is one immutable
 * value swapped atomically. Intents run in [context] (the main thread in an app).
 */
class LsDoRegistry(
    private val json: Json = LsDoJson,
    private val context: CoroutineContext = EmptyCoroutineContext,
    private val settle: Duration = 300.milliseconds,
    private val timeout: Duration = 10.seconds,
    /** Reads what is rendered, one tree per window (null: this build cannot). Runs in [context]. */
    private val capture: (suspend () -> List<UiNode>)? = null,
) : LsDo.Sink {
    private class Entry(val id: String, val screen: LsDoScreen<Any, Any>)

    private val entries = MutableStateFlow<List<Entry>>(emptyList())

    @Suppress("UNCHECKED_CAST")
    override fun register(name: String, screen: LsDoScreen<*, *>): () -> Unit {
        var added: Entry? = null
        entries.update { current ->
            val taken = current.map { it.id }.toSet()
            val id = generateSequence(1) { it + 1 }.map { if (it == 1) name else "$name-$it" }.first { it !in taken }
            Entry(id, screen as LsDoScreen<Any, Any>).also { added = it }.let { current + it }
        }
        return { entries.update { current -> current.filterNot { it === added } } }
    }

    val ids: List<String> get() = entries.value.map { it.id }

    /** Every screen on show: its state and the actions available in it. */
    fun ls(): JsonObject = buildJsonObject {
        put("screens", JsonArray(entries.value.map(::describe)))
    }

    /** Whether [ui] can read the rendered UI in this build. */
    val canCaptureUi: Boolean get() = capture != null

    /** What is rendered now, without its empty wrappers; null when this build cannot read it. */
    suspend fun ui(): List<UiNode>? = capture?.let { read -> withContext(context) { read() }.flatMap { it.pruned() } }

    /** One screen, or null when none has that id. */
    fun ls(id: String): JsonObject? = find(id)?.let(::describe)

    /**
     * Runs [intent] (`{"type": "<action>", ...params}`) on screen [id], waits until the screen
     * is idle (not busy, no change for [settle]) or [timeout] passed, then lists every screen.
     */
    suspend fun perform(id: String, intent: JsonElement, withUi: Boolean = false): LsDoOutcome {
        val entry = find(id) ?: return failure("No screen \"$id\". Screens: ${ids.joinToString().ifEmpty { "none" }}.")
        val screen = entry.screen
        val decoded = try {
            json.decodeFromJsonElement(screen.lsdo.intent, intent)
        } catch (e: SerializationException) {
            return failure("Not an action of \"$id\": ${e.message}")
        } catch (e: IllegalArgumentException) {
            return failure("Not an action of \"$id\": ${e.message}")
        }
        val name = (intent as? JsonObject)?.get("type")?.let { (it as? JsonPrimitive)?.content }
        val available = screen.availableIntents(screen.state.value)
        if (available != null && name != null && name !in available) {
            return failure("\"$name\" is not available on \"$id\" now. Available: ${available.joinToString()}.")
        }
        val message = try {
            withContext(context) { screen.perform(decoded) }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            return failure("\"$name\" failed: ${e.message ?: e::class.simpleName}")
        }
        val idle = idle(screen)
        val note = message ?: if (idle) null else "Still busy after $timeout: ls again to see where it got to."
        val ui = if (withUi) ui()?.render() else null
        return LsDoOutcome(ok = message == null, message = note, screens = ls(), ui = ui)
    }

    /**
     * Gives the intent [settle] to start its work, then waits until the screen is not busy.
     * Changes that are not work (download progress) do not hold the answer back. False: timed out.
     */
    private suspend fun idle(screen: LsDoScreen<Any, Any>): Boolean = withTimeoutOrNull(timeout) {
        delay(settle)
        screen.state.first { !screen.isBusy(it) }
    } != null

    private fun failure(message: String) = LsDoOutcome(ok = false, message = message, screens = ls())

    private fun find(id: String): Entry? = entries.value.firstOrNull { it.id == id }

    private fun describe(entry: Entry): JsonObject {
        val screen = entry.screen
        val state = screen.state.value
        val available = screen.availableIntents(state)
        val all = actionsOf(screen.lsdo.intent.descriptor)
        val actions = all.filter { available == null || it.name in available }
        return buildJsonObject {
            put("id", entry.id)
            put("busy", screen.isBusy(state))
            put("state", json.encodeToJsonElement(screen.lsdo.state, state))
            put(
                "actions",
                JsonArray(
                    actions.map { action ->
                        buildJsonObject {
                            put("name", action.name)
                            action.description?.let { put("description", it) }
                            put("params", action.params)
                        }
                    },
                ),
            )
            // What exists on this screen but makes no sense in this state (inside a folder only, etc.).
            val unavailable = all.map { it.name }.filterNot { name -> actions.any { it.name == name } }
            if (unavailable.isNotEmpty()) put("unavailable", JsonArray(unavailable.map(::JsonPrimitive)))
        }
    }
}
