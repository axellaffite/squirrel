package app.squirrel.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import app.squirrel.lsdo.LsDo
import app.squirrel.lsdo.LsDoScreen

/** Lists [screen] for an agent (LS-DO) while it is composed; a no-op unless a debug build installed the runtime. */
@Composable
fun RegisterLsDo(name: String, screen: LsDoScreen<*, *>) {
    DisposableEffect(name, screen) {
        val remove = LsDo.register(name, screen)
        onDispose { remove() }
    }
}
