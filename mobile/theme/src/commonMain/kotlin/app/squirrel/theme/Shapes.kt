package app.squirrel.theme

import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Shape

/** MASTER.md "Radii". */
object Shapes {
    /** Chips, badges, segment tracks, toggles, avatars, the phone bar and its centre button. */
    val pill: Shape = RoundedCornerShape(percent = 50)

    /** Buttons, fields, icon buttons and icon tiles (`--radius-control`). */
    val control: Shape = RoundedCornerShape(Tokens.radiusControl)
    val circle: Shape = CircleShape
    val small: Shape = RoundedCornerShape(Tokens.radiusSm)
    val tile: Shape = RoundedCornerShape(Tokens.radiusTile)
    val card: Shape = RoundedCornerShape(Tokens.radiusCard)
    val well: Shape = RoundedCornerShape(Tokens.radiusWell)
    val panel: Shape = RoundedCornerShape(Tokens.radiusPanel)
    val rail: Shape = RoundedCornerShape(Tokens.radiusRail)
}
