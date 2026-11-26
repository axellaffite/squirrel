package app.squirrel.theme

import androidx.compose.ui.window.DialogProperties

/**
 * A full-screen, edge-to-edge dialog window for sheets: the scrim covers the whole screen and the
 * sheet reaches the bottom edge (it pads its own content by the navigation bar inset). Each platform
 * names the switch differently, hence one function per platform.
 */
expect fun edgeToEdgeDialogProperties(): DialogProperties
