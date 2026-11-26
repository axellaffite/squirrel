package app.squirrel.theme

import android.os.Build

/** `Modifier.blur` needs RenderEffect, from Android 12 (API 31). */
actual fun platformBlursBackdrop(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
