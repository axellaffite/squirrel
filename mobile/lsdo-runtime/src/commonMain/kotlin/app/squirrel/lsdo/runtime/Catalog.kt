package app.squirrel.lsdo.runtime

import app.squirrel.lsdo.LsDoEntry
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Every screen in a few words: name, what it is, its action names (and whether it is on show, given [shown]). */
fun catalogIndex(entries: List<LsDoEntry>, shown: Set<String>? = null): JsonObject = buildJsonObject {
    put(
        "screens",
        JsonArray(
            entries.map { entry ->
                buildJsonObject {
                    put("name", entry.name)
                    put("description", entry.description)
                    put("actions", JsonArray(actionsOf(entry.spec.intent.descriptor).map { JsonPrimitive(it.name) }))
                    if (shown != null) put("shown", entry.name in shown)
                }
            },
        ),
    )
}

/** One screen in full: each action with its meaning and parameters, and the shape of its state. */
fun catalogScreen(entry: LsDoEntry): JsonObject = buildJsonObject {
    put("name", entry.name)
    put("description", entry.description)
    put(
        "actions",
        JsonArray(
            actionsOf(entry.spec.intent.descriptor).map { action ->
                buildJsonObject {
                    put("name", action.name)
                    action.description?.let { put("description", it) }
                    put("params", action.params)
                }
            },
        ),
    )
    put("state", schemaOf(entry.spec.state.descriptor))
}

/** The whole catalog, every screen in full: what `lsdo-catalog.json` holds. */
fun catalogFull(entries: List<LsDoEntry>): JsonObject = buildJsonObject {
    put("screens", JsonArray(entries.map(::catalogScreen)))
}
