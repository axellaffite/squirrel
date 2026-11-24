package app.squirrel.lsdo.runtime

import kotlinx.serialization.Serializable

/** A rectangle on screen, in pixels from the window's top left. */
@Serializable
data class UiBounds(val x: Int, val y: Int, val width: Int, val height: Int)

/**
 * What is rendered, platform-neutral: Compose semantics on Android, the UIKit accessibility
 * elements Compose builds from them on iOS. [flags] are states (disabled, selected, checked,
 * focused, heading); [actions] what a user can do on it (click, scroll, type).
 */
@Serializable
data class UiNode(
    val role: String? = null,
    val text: String? = null,
    val value: String? = null,
    val flags: List<String> = emptyList(),
    val actions: List<String> = emptyList(),
    val bounds: UiBounds? = null,
    val children: List<UiNode> = emptyList(),
) {
    /** Says something on its own: a role, words, a state or an action. */
    val meaningful: Boolean
        get() = role != null || !text.isNullOrBlank() || !value.isNullOrBlank() || flags.isNotEmpty() || actions.isNotEmpty()
}

/** The tree without its empty wrappers: a node with nothing to say gives way to its children. */
fun UiNode.pruned(): List<UiNode> {
    val kids = children.flatMap { it.pruned() }
    return if (meaningful) listOf(copy(children = kids)) else kids
}

/**
 * The compact text an agent reads, one node per line, indented by depth:
 * `button "Add to Documents" [click]`, `text "Taxes 2026" {selected}`, `@x,y wxh` with [bounds].
 */
fun List<UiNode>.render(bounds: Boolean = false): String = buildString {
    fun line(node: UiNode, depth: Int) {
        append("  ".repeat(depth))
        append(node.role ?: "node")
        node.text?.takeIf { it.isNotBlank() }?.let { append(" \"").append(it.replace("\n", " ")).append('"') }
        node.value?.takeIf { it.isNotBlank() }?.let { append(" = \"").append(it.replace("\n", " ")).append('"') }
        if (node.flags.isNotEmpty()) append(node.flags.joinToString(",", " {", "}"))
        if (node.actions.isNotEmpty()) append(node.actions.joinToString(",", " [", "]"))
        if (bounds) node.bounds?.let { append(" @${it.x},${it.y} ${it.width}x${it.height}") }
        append('\n')
        node.children.forEach { line(it, depth + 1) }
    }
    this@render.forEach { line(it, 0) }
}.trimEnd()
