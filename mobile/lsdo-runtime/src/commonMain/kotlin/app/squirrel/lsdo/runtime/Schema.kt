package app.squirrel.lsdo.runtime

import app.squirrel.lsdo.Doc
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.descriptors.PolymorphicKind
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.SerialKind
import kotlinx.serialization.descriptors.StructureKind
import kotlinx.serialization.descriptors.elementDescriptors
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** One thing a screen can do: its name (the intent's serial name), meaning and parameters. */
data class LsDoAction(val name: String, val description: String?, val params: JsonObject)

/** The actions of an intent type: one per subclass of a sealed type, else the type itself. */
@OptIn(ExperimentalSerializationApi::class)
fun actionsOf(intent: SerialDescriptor, seen: Set<String> = emptySet()): List<LsDoAction> {
    val cases = if (intent.kind == PolymorphicKind.SEALED) intent.getElementDescriptor(1).elementDescriptors.toList() else listOf(intent)
    return cases.map { LsDoAction(it.serialName, it.annotations.doc(), schemaOf(it, seen)) }
}

/**
 * A JSON Schema (the subset an agent needs) read from a serializer's descriptor: no reflection,
 * so it works on every target. A type met again inside itself becomes a `$ref` to its name.
 */
@OptIn(ExperimentalSerializationApi::class)
fun schemaOf(descriptor: SerialDescriptor, seen: Set<String> = emptySet()): JsonObject {
    val base = when (val kind = descriptor.kind) {
        is PrimitiveKind -> primitive(kind)
        SerialKind.ENUM -> buildJsonObject {
            put("type", "string")
            put("enum", JsonArray((0 until descriptor.elementsCount).map { JsonPrimitive(descriptor.getElementName(it)) }))
        }
        StructureKind.LIST -> buildJsonObject {
            put("type", "array")
            put("items", schemaOf(descriptor.getElementDescriptor(0), seen))
        }
        StructureKind.MAP -> buildJsonObject {
            put("type", "object")
            put("additionalProperties", schemaOf(descriptor.getElementDescriptor(1), seen))
        }
        StructureKind.CLASS, StructureKind.OBJECT -> when {
            descriptor.isInline -> schemaOf(descriptor.getElementDescriptor(0), seen)
            descriptor.serialName in seen -> buildJsonObject { put("\$ref", descriptor.serialName) }
            else -> objectSchema(descriptor, seen + descriptor.serialName)
        }
        PolymorphicKind.SEALED -> buildJsonObject {
            put("oneOf", JsonArray(actionsOf(descriptor, seen).map { it.params }))
        }
        PolymorphicKind.OPEN, SerialKind.CONTEXTUAL -> JsonObject(emptyMap())
    }
    return if (descriptor.isNullable) JsonObject(base + ("nullable" to JsonPrimitive(true))) else base
}

@OptIn(ExperimentalSerializationApi::class)
private fun objectSchema(descriptor: SerialDescriptor, seen: Set<String>): JsonObject = buildJsonObject {
    put("type", "object")
    val names = (0 until descriptor.elementsCount)
    put(
        "properties",
        buildJsonObject {
            names.forEach { i ->
                val property = schemaOf(descriptor.getElementDescriptor(i), seen)
                val doc = descriptor.getElementAnnotations(i).doc()
                put(descriptor.getElementName(i), if (doc == null) property else JsonObject(property + ("description" to JsonPrimitive(doc))))
            }
        },
    )
    val required = names.filterNot(descriptor::isElementOptional).map(descriptor::getElementName)
    if (required.isNotEmpty()) put("required", buildJsonArray { required.forEach { add(JsonPrimitive(it)) } })
}

private fun primitive(kind: PrimitiveKind): JsonObject {
    val type = when (kind) {
        PrimitiveKind.BOOLEAN -> "boolean"
        PrimitiveKind.BYTE, PrimitiveKind.SHORT, PrimitiveKind.INT, PrimitiveKind.LONG -> "integer"
        PrimitiveKind.FLOAT, PrimitiveKind.DOUBLE -> "number"
        PrimitiveKind.CHAR, PrimitiveKind.STRING -> "string"
    }
    return buildJsonObject { put("type", type) }
}

private fun List<Annotation>.doc(): String? = filterIsInstance<Doc>().firstOrNull()?.text
