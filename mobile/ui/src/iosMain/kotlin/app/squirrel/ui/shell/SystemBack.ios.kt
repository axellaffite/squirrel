package app.squirrel.ui.shell

import androidx.compose.runtime.Composable

/** iOS has no system back; the header's back button is the way out (swipe back: Milestone H). */
@Composable
actual fun SystemBack(enabled: Boolean, onBack: () -> Unit) = Unit
