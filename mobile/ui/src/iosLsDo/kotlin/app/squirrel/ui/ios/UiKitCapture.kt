package app.squirrel.ui.ios

import app.squirrel.lsdo.runtime.UiBounds
import app.squirrel.lsdo.runtime.UiNode
import kotlinx.cinterop.ExperimentalForeignApi
import kotlinx.cinterop.useContents
import kotlinx.coroutines.delay
import platform.darwin.NSObject
import platform.UIKit.UIAccessibilityTraitAdjustable
import platform.UIKit.UIAccessibilityTraitButton
import platform.UIKit.UIAccessibilityTraitHeader
import platform.UIKit.UIAccessibilityTraitImage
import platform.UIKit.UIAccessibilityTraitLink
import platform.UIKit.UIAccessibilityTraitNotEnabled
import platform.UIKit.UIAccessibilityTraitSearchField
import platform.UIKit.UIAccessibilityTraitSelected
import platform.UIKit.UIAccessibilityTraitStaticText
import platform.UIKit.UIAccessibilityTraitTabBar
import platform.UIKit.UIAccessibilityTraitUpdatesFrequently
import platform.UIKit.UIApplication
import platform.UIKit.UIView
import platform.UIKit.UIWindow
import platform.UIKit.accessibilityElementAtIndex
import platform.UIKit.accessibilityElementCount
import platform.UIKit.accessibilityFrame
import platform.UIKit.accessibilityLabel
import platform.UIKit.accessibilityTraits
import platform.UIKit.accessibilityValue
import platform.UIKit.isAccessibilityElement
import kotlin.math.roundToInt

/** Elements nested deeper than this are cut (a cycle in a broken container would never end). */
private const val MAX_DEPTH = 64

/**
 * Reads what is rendered on iOS as LS-DO [UiNode]s: Compose Multiplatform turns its semantics into
 * UIKit accessibility elements (what VoiceOver and XCUITest see), read here through public UIKit
 * API. Bounds are in points, not pixels. Main thread only.
 */
internal object UiKitCapture {
    @Suppress("DEPRECATION")
    suspend fun capture(): List<UiNode> {
        // The state an intent changed is drawn on the next frames.
        delay(50)
        return UIApplication.sharedApplication.windows
            .filterIsInstance<UIWindow>()
            .filterNot { it.hidden }
            .map { UiNode(role = "window", children = children(it, 0)) }
            .filter { it.children.isNotEmpty() }
    }

    private fun children(container: NSObject, depth: Int): List<UiNode> {
        if (depth > MAX_DEPTH) return emptyList()
        val count = container.accessibilityElementCount().toInt()
        val elements = if (count > 0) {
            (0 until count).mapNotNull { container.accessibilityElementAtIndex(it.toLong()) as? NSObject }
        } else {
            (container as? UIView)?.subviews?.filterIsInstance<UIView>()?.filterNot { it.hidden }.orEmpty()
        }
        return elements.mapNotNull { node(it, depth + 1) }
    }

    @OptIn(ExperimentalForeignApi::class)
    private fun node(element: NSObject, depth: Int): UiNode? {
        val kids = if (element.isAccessibilityElement()) emptyList() else children(element, depth)
        if (!element.isAccessibilityElement()) return if (kids.isEmpty()) null else UiNode(children = kids)
        val traits = element.accessibilityTraits
        val bounds = element.accessibilityFrame.useContents {
            UiBounds(origin.x.roundToInt(), origin.y.roundToInt(), size.width.roundToInt(), size.height.roundToInt())
        }
        val value = element.accessibilityValue
        return UiNode(
            // Compose gives its text fields no trait: an element holding a value is one.
            role = role(traits) ?: if (value != null) "textfield" else null,
            text = element.accessibilityLabel,
            value = value,
            flags = buildList {
                if (traits has UIAccessibilityTraitNotEnabled) add("disabled")
                if (traits has UIAccessibilityTraitSelected) add("selected")
            },
            actions = buildList {
                if (traits has UIAccessibilityTraitButton || traits has UIAccessibilityTraitLink) add("click")
                if (traits has UIAccessibilityTraitAdjustable) add("adjust")
            },
            bounds = bounds,
            children = kids,
        )
    }

    private fun role(traits: ULong): String? = when {
        traits has UIAccessibilityTraitSearchField -> "textfield"
        traits has UIAccessibilityTraitHeader -> "heading"
        traits has UIAccessibilityTraitTabBar -> "tabs"
        traits has UIAccessibilityTraitButton -> "button"
        traits has UIAccessibilityTraitLink -> "link"
        traits has UIAccessibilityTraitImage -> "image"
        traits has UIAccessibilityTraitAdjustable -> "picker"
        traits has UIAccessibilityTraitUpdatesFrequently -> "progress"
        traits has UIAccessibilityTraitStaticText -> "text"
        else -> null
    }

    private infix fun ULong.has(trait: ULong): Boolean = this and trait != 0uL
}
