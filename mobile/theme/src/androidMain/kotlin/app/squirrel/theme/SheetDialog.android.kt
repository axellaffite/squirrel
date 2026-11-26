package app.squirrel.theme

import androidx.compose.ui.window.DialogProperties

/** Android: the dialog window stops fitting the system bars. */
actual fun edgeToEdgeDialogProperties(): DialogProperties =
    DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)
