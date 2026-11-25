package app.squirrel.ui.shell

import io.github.axellaffite.lsdo.runtime.LsDoRegistry
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.time.Duration.Companion.milliseconds

class ShellLsDoTest {
    @Test
    fun goToSelectsTheTabTheWayTheBottomBarDoes() = runTest {
        val selected = mutableListOf<AppTab>()
        lateinit var shell: ShellLsDo
        shell = ShellLsDo(AppTab.Photos) { tab -> selected += tab; shell.shown(tab) }
        val registry = LsDoRegistry(settle = 10.milliseconds)
        registry.register("app", shell)

        val done = registry.perform("app", Json.parseToJsonElement("""{"type":"go_to","tab":"files"}"""))

        assertEquals(listOf(AppTab.Files), selected)
        val app = done.screens.getValue("screens").jsonArray.single().jsonObject
        assertEquals("files", app.getValue("state").jsonObject.getValue("tab").jsonPrimitive.content)
    }

    @Test
    fun theTabParameterListsEverySection() {
        val registry = LsDoRegistry()
        registry.register("app", ShellLsDo(AppTab.Photos) {})

        val goTo = requireNotNull(registry.ls("app")).getValue("actions").jsonArray.single().jsonObject
        val tabs = goTo.getValue("params").jsonObject.getValue("properties").jsonObject.getValue("tab").jsonObject.getValue("enum").jsonArray

        assertEquals(listOf("photos", "files", "shared", "backup"), tabs.map { it.jsonPrimitive.content })
    }
}
