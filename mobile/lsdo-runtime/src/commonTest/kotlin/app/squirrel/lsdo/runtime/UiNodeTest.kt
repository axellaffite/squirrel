package app.squirrel.lsdo.runtime

import kotlin.test.Test
import kotlin.test.assertEquals

class UiNodeTest {
    private val tree = UiNode(
        children = listOf(
            UiNode(role = "heading", text = "Documents", flags = listOf("heading")),
            UiNode(
                children = listOf(
                    UiNode(role = "button", text = "Add to Documents", actions = listOf("click"), bounds = UiBounds(10, 20, 48, 48)),
                    UiNode(children = listOf(UiNode(text = "Taxes 2026", flags = listOf("selected")))),
                ),
            ),
            UiNode(role = "textfield", text = "Name", value = "Tax\nes", actions = listOf("type")),
        ),
    )

    @Test
    fun `empty wrappers give way to their children`() {
        val pruned = tree.pruned()

        assertEquals(listOf("Documents", "Add to Documents", "Taxes 2026", "Name"), pruned.map { it.text })
    }

    @Test
    fun `the text form is one line per node with states actions and values`() {
        val expected = """
            heading "Documents" {heading}
            button "Add to Documents" [click]
            node "Taxes 2026" {selected}
            textfield "Name" = "Tax es" [type]
        """.trimIndent()

        assertEquals(expected, tree.pruned().render())
    }

    @Test
    fun `bounds are added only when asked`() {
        val button = listOf(UiNode(role = "button", text = "Ok", bounds = UiBounds(10, 20, 48, 48)))

        assertEquals("button \"Ok\" @10,20 48x48", button.render(bounds = true))
        assertEquals("button \"Ok\"", button.render())
    }

    @Test
    fun `children are indented under their parent`() {
        val list = listOf(UiNode(role = "list", children = listOf(UiNode(text = "a"), UiNode(text = "b"))))

        assertEquals("list\n  node \"a\"\n  node \"b\"", list.render())
    }
}
