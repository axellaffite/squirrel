package app.squirrel.theme

import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Shape

/** MASTER.md "Radii". */
object Shapes {
    /** Every button, field, segmented control, badge and icon well. */
    val pill: Shape = RoundedCornerShape(percent = 50)
    val circle: Shape = CircleShape
    val small: Shape = RoundedCornerShape(Tokens.radiusSm)
    val tile: Shape = RoundedCornerShape(Tokens.radiusTile)
    val card: Shape = RoundedCornerShape(Tokens.radiusCard)
    val well: Shape = RoundedCornerShape(Tokens.radiusWell)
    val panel: Shape = RoundedCornerShape(Tokens.radiusPanel)
    val rail: Shape = RoundedCornerShape(Tokens.radiusRail)
}
