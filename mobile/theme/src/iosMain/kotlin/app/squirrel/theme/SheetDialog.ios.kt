package app.squirrel.theme

import androidx.compose.ui.window.DialogProperties

/** iOS and desktop: the dialog is not inset by the safe area (on iOS it stopped above the home indicator). */
actual fun edgeToEdgeDialogProperties(): DialogProperties =
    DialogProperties(usePlatformDefaultWidth = false, usePlatformInsets = false)
