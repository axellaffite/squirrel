package app.squirrel.lsdo.runtime

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.serializer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class SchemaTest {
    @Serializable
    enum class Color { Red, Green }

    @Serializable
    data class Tree(val label: String, val color: Color, val children: List<Tree> = emptyList(), val sizes: Map<String, Double> = emptyMap())

    @Test
    fun `a sealed intent gives one action per subclass with its doc and parameters`() {
        val actions = actionsOf(serializer<CounterIntent>().descriptor)

        assertEquals(listOf("add", "fail", "reset"), actions.map { it.name })
        val add = actions.first()
        assertEquals("Adds n to the count", add.description)
        assertEquals("integer", add.params.prop("n")["type"]?.jsonPrimitive?.content)
        assertEquals("true", add.params.prop("note")["nullable"]?.jsonPrimitive?.content)
        assertEquals(listOf("n"), add.params["required"]?.jsonArray?.map { it.jsonPrimitive.content })
        assertNull(actions.single { it.name == "reset" }.params["required"])
    }

    @Test
    fun `enums lists maps and recursion are described`() {
        val schema = schemaOf(serializer<Tree>().descriptor)

        assertEquals(listOf("Red", "Green"), schema.prop("color")["enum"]?.jsonArray?.map { it.jsonPrimitive.content })
        val items = schema.prop("children")["items"]?.jsonObject
        assertEquals(serializer<Tree>().descriptor.serialName, items?.get("\$ref")?.jsonPrimitive?.content)
        assertEquals("number", schema.prop("sizes")["additionalProperties"]?.jsonObject?.get("type")?.jsonPrimitive?.content)
        assertEquals(listOf("label", "color"), schema["required"]?.jsonArray?.map { it.jsonPrimitive.content })
    }

    @Test
    fun `a documented state field carries its description`() {
        val schema = schemaOf(serializer<CounterState>().descriptor)

        assertEquals("The current count", schema.prop("count")["description"]?.jsonPrimitive?.content)
    }

    private fun JsonObject.prop(name: String): JsonObject = getValue("properties").jsonObject.getValue(name).jsonObject
}
