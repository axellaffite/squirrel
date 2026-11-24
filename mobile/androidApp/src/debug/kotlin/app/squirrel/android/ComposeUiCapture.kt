package app.squirrel.android

import android.view.View
import androidx.compose.ui.platform.ViewRootForTest
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.state.ToggleableState
import app.squirrel.lsdo.runtime.UiBounds
import app.squirrel.lsdo.runtime.UiNode
import kotlinx.coroutines.android.awaitFrame
import java.lang.ref.WeakReference
import kotlin.math.roundToInt

/**
 * Reads what Compose renders, as LS-DO [UiNode]s: the merged semantics tree (what TalkBack and
 * UI tests see) of every Compose root shown, the activity's and each dialog's or popup's window.
 * Main thread only: roots are recorded as Compose creates them and read on the main thread.
 */
internal object ComposeUiCapture {
    private var roots: List<WeakReference<ViewRootForTest>> = emptyList()

    /** Before the first Compose view exists (Application.onCreate). */
    fun install() {
        ViewRootForTest.onViewCreatedCallback = { root ->
            roots = roots.filter { it.get() != null } + WeakReference(root)
        }
    }

    /** One tree per window shown, oldest first (dialogs come after the screen they cover). */
    suspend fun capture(): List<UiNode> {
        // The state an intent changed is drawn on the next frame.
        awaitFrame()
        return roots.mapNotNull { it.get() }
            .filter { it.view.isAttachedToWindow && it.view.windowVisibility == View.VISIBLE }
            .map { root -> UiNode(role = "window", children = root.semanticsOwner.rootSemanticsNode.children.mapNotNull(::node)) }
    }

    private fun node(semantics: SemanticsNode): UiNode? {
        val config = semantics.config
        if (config.getOrNull(SemanticsProperties.HideFromAccessibility) != null) return null
        val bounds = semantics.boundsInWindow
        val children = semantics.children.mapNotNull(::node)
        if ((bounds.width <= 0f || bounds.height <= 0f) && children.isEmpty()) return null
        val password = config.getOrNull(SemanticsProperties.Password) != null
        val editable = config.getOrNull(SemanticsProperties.EditableText)
        val words = config.getOrNull(SemanticsProperties.ContentDescription)?.joinToString(" ")
            ?: config.getOrNull(SemanticsProperties.Text)?.joinToString(" ") { it.text }
        val progress = config.getOrNull(SemanticsProperties.ProgressBarRangeInfo)?.let { info ->
            val span = info.range.endInclusive - info.range.start
            if (span > 0f) "${((info.current - info.range.start) / span * 100).roundToInt()}%" else null
        }
        return UiNode(
            role = role(semantics, editable != null),
            text = words,
            value = when {
                editable != null -> if (password && editable.text.isNotEmpty()) "••••" else editable.text
                else -> config.getOrNull(SemanticsProperties.StateDescription) ?: progress
            },
            flags = flags(semantics),
            actions = actions(semantics),
            bounds = UiBounds(bounds.left.roundToInt(), bounds.top.roundToInt(), bounds.width.roundToInt(), bounds.height.roundToInt()),
            children = children,
        )
    }

    private fun role(semantics: SemanticsNode, editable: Boolean): String? {
        val config = semantics.config
        return when (config.getOrNull(SemanticsProperties.Role)) {
            Role.Button -> "button"
            Role.Checkbox -> "checkbox"
            Role.Switch -> "switch"
            Role.RadioButton -> "radio"
            Role.Tab -> "tab"
            Role.Image -> "image"
            Role.DropdownList -> "dropdown"
            Role.ValuePicker -> "picker"
            else -> when {
                editable -> "textfield"
                config.getOrNull(SemanticsProperties.Heading) != null -> "heading"
                config.getOrNull(SemanticsProperties.ProgressBarRangeInfo) != null -> "progress"
                config.getOrNull(SemanticsProperties.CollectionInfo) != null -> "list"
                config.getOrNull(SemanticsProperties.Text) != null -> "text"
                config.getOrNull(SemanticsActions.OnClick) != null -> "clickable"
                else -> null
            }
        }
    }

    private fun flags(semantics: SemanticsNode): List<String> = buildList {
        val config = semantics.config
        if (config.getOrNull(SemanticsProperties.Disabled) != null) add("disabled")
        if (config.getOrNull(SemanticsProperties.Selected) == true) add("selected")
        when (config.getOrNull(SemanticsProperties.ToggleableState)) {
            ToggleableState.On -> add("checked")
            ToggleableState.Indeterminate -> add("mixed")
            else -> Unit
        }
        if (config.getOrNull(SemanticsProperties.Focused) == true) add("focused")
        if (config.getOrNull(SemanticsProperties.Password) != null) add("password")
        config.getOrNull(SemanticsProperties.Error)?.let { add("error: $it") }
    }

    private fun actions(semantics: SemanticsNode): List<String> = buildList {
        val config = semantics.config
        if (config.getOrNull(SemanticsActions.OnClick) != null) add("click")
        if (config.getOrNull(SemanticsActions.OnLongClick) != null) add("longclick")
        if (config.getOrNull(SemanticsActions.ScrollBy) != null) add("scroll")
        if (config.getOrNull(SemanticsActions.SetText) != null) add("type")
        if (config.getOrNull(SemanticsActions.Dismiss) != null) add("dismiss")
    }
}
