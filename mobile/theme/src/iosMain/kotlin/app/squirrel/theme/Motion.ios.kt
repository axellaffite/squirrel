package app.squirrel.theme

import androidx.compose.runtime.Composable
import platform.UIKit.UIAccessibilityIsReduceMotionEnabled

@Composable
actual fun systemReducesMotion(): Boolean = UIAccessibilityIsReduceMotionEnabled()
