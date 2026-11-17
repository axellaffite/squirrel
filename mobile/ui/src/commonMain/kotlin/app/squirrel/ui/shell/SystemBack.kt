package app.squirrel.ui.shell

import androidx.compose.runtime.Composable

/** The system back gesture or button leaves a sub-page (Android); a no-op where there is none. */
@Composable
expect fun SystemBack(enabled: Boolean = true, onBack: () -> Unit)
