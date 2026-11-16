package app.squirrel.shared.api

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/** The hand-written calls against the same openapi.json the generated client comes from. */
class ContractTest {
    private val spec: JsonObject by lazy {
        val path = checkNotNull(System.getProperty("openapi.spec")) { "openapi.spec is not set by Gradle" }
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }

    private fun operation(path: String, method: String): JsonObject =
        checkNotNull(spec["paths"]?.jsonObject?.get(path)?.jsonObject?.get(method)?.jsonObject) {
            "openapi.json has no $method $path"
        }

    private fun schemes(op: JsonObject): Set<String> =
        (op["security"] as JsonArray).flatMap { it.jsonObject.keys }.toSet()

    @Test
    fun theChunkPutTakesAnOctetStreamUnderTheDeviceBearer() {
        val op = operation(CHUNK_PATH, "put")
        val success = op["responses"]!!.jsonObject.keys.filter { it.startsWith("2") }
        assertEquals(listOf(CHUNK_SUCCESS.toString()), success)
        val index = op["parameters"]!!.jsonArray.map { it.jsonObject }.single { it["name"]!!.jsonPrimitive.content == "index" }
        assertEquals("integer", index["schema"]!!.jsonObject["type"]!!.jsonPrimitive.content)
        val content = op["requestBody"]!!.jsonObject["content"]!!.jsonObject
        assertEquals(setOf("application/octet-stream"), content.keys)
        assertTrue("device_bearer" in schemes(op))
        val params = op["parameters"]!!.jsonArray.map { it.jsonObject["name"]!!.jsonPrimitive.content }
        assertEquals(listOf("id", "index"), params)
    }

    @OptIn(ExperimentalSerializationApi::class)
    @Test
    fun theUploadBodyMatchesCreateUploadRequest() {
        val schema = spec["components"]!!.jsonObject["schemas"]!!.jsonObject["CreateUploadRequest"]!!.jsonObject
        val properties = schema["properties"]!!.jsonObject.keys
        val descriptor = CreateUploadBody.serializer().descriptor
        val ours = (0 until descriptor.elementsCount).map(descriptor::getElementName).toSet()
        assertEquals(properties, ours)
        val required = schema["required"]!!.jsonArray.map { it.jsonPrimitive.content }.toSet()
        val ourRequired = (0 until descriptor.elementsCount)
            .filterNot(descriptor::isElementOptional)
            .map(descriptor::getElementName)
            .toSet()
        assertEquals(required, ourRequired)
    }

    @Test
    fun createUploadAnswersTheTwoBodiesTheFacadeDecodes() {
        val responses = operation("/api/uploads", "post")["responses"]!!.jsonObject
        fun ref(status: String) = responses[status]!!.jsonObject["content"]!!.jsonObject["application/json"]!!
            .jsonObject["schema"]!!.jsonObject["\$ref"]!!.jsonPrimitive.content
        assertEquals("#/components/schemas/DeduplicatedUpload", ref("200"))
        assertEquals("#/components/schemas/UploadSessionDto", ref("201"))
    }
}
