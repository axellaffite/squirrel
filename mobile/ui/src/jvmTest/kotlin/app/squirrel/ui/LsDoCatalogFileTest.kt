package app.squirrel.ui

import app.squirrel.lsdo.runtime.catalogFull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals

/**
 * `mobile/lsdo-catalog.json` is every LS-DO screen in full, for planning a scenario without a
 * device. It is generated from [LsDoScreens]: `./gradlew :ui:lsdoCatalog` writes it, and this
 * test fails when the committed copy no longer matches the code.
 */
class LsDoCatalogFileTest {
    private val json = Json { prettyPrint = true }

    @Test
    fun theCommittedCatalogMatchesTheCode() {
        val file = File(requireNotNull(System.getProperty("lsdo.catalog")) { "lsdo.catalog is set by the build" })
        val generated = json.encodeToString(JsonElement.serializer(), catalogFull(LsDoScreens)) + "\n"
        if (System.getProperty("lsdo.catalog.write") == "true") {
            file.writeText(generated)
            return
        }
        val committed = if (file.exists()) file.readText() else ""
        assertEquals(generated, committed, "mobile/lsdo-catalog.json is stale: run ./gradlew :ui:lsdoCatalog")
    }
}
