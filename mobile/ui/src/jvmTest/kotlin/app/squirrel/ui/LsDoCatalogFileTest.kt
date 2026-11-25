package app.squirrel.ui

import io.github.axellaffite.lsdo.runtime.catalogIndex
import io.github.axellaffite.lsdo.runtime.catalogScreen
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals

/**
 * `mobile/lsdo-catalog/` is every LS-DO screen, for planning a scenario without a device:
 * `index.json` (names, how to reach them, action names) and one `<screen>.json` in full each, so
 * an agent reads the index and only the screens it needs. Generated from [LsDoScreens]:
 * `./gradlew :ui:lsdoCatalog` writes it, and this test fails when the committed copy is stale.
 */
class LsDoCatalogFileTest {
    private val json = Json { prettyPrint = true }

    private fun text(element: JsonElement) = json.encodeToString(JsonElement.serializer(), element) + "\n"

    @Test
    fun theCommittedCatalogMatchesTheCode() {
        val dir = File(requireNotNull(System.getProperty("lsdo.catalog")) { "lsdo.catalog is set by the build" })
        val generated = mapOf("index.json" to text(catalogIndex(LsDoScreens))) +
            LsDoScreens.associate { "${it.name}.json" to text(catalogScreen(it)) }
        if (System.getProperty("lsdo.catalog.write") == "true") {
            dir.deleteRecursively()
            dir.mkdirs()
            generated.forEach { (name, content) -> dir.resolve(name).writeText(content) }
            return
        }
        val committed = dir.listFiles().orEmpty().associate { it.name to it.readText() }
        assertEquals(generated, committed, "mobile/lsdo-catalog/ is stale: run ./gradlew :ui:lsdoCatalog")
    }
}
