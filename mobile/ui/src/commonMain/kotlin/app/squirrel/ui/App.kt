package app.squirrel.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.semantics.clearAndSetSemantics
import app.squirrel.ui.shell.LocalOverlay
import app.squirrel.ui.shell.OverlayHost
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.ui.Modifier
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.components.ground
import app.squirrel.ui.backup.BackupTab
import app.squirrel.ui.images.LocalImageLoader
import app.squirrel.ui.photos.PhotosTab
import app.squirrel.ui.files.FilesTab
import app.squirrel.ui.backup.PhotoAccessActions
import app.squirrel.ui.shell.Shell
import app.squirrel.ui.signin.SignInFlow

/** The app's root composable, shared by Android and iOS. */
@Composable
fun App(graph: AppGraph, actions: PhotoAccessActions = PhotoAccessActions.None) {
    SquirrelTheme {
        val state by graph.state.collectAsStateWithLifecycle()
        val images by graph.imageLoader.collectAsStateWithLifecycle()
        CompositionLocalProvider(LocalImageLoader provides images) {
            Box(Modifier.fillMaxSize().ground()) {
                when (val current = state) {
                    AppState.Loading -> Unit
                    is AppState.NeedsSignIn -> SignInFlow(graph, current.attempt, current.serverUrl)
                    is AppState.SignedIn -> {
                        // One overlay per session: it goes with the session's view models.
                        val overlay = remember(current.scope) { OverlayHost() }
                        CompositionLocalProvider(
                            LocalViewModelStoreOwner provides current.scope,
                            LocalOverlay provides overlay,
                        ) {
                            // Under the full-screen layer, the shell is hidden from assistive tech.
                            val covered = overlay.content != null
                            Box(Modifier.fillMaxSize().then(if (covered) Modifier.clearAndSetSemantics {} else Modifier)) {
                                Shell(
                                    current.account,
                                    onSignOut = graph::signOut,
                                    onRefresh = graph::refreshAccount,
                                    backup = { BackupTab(graph, actions) },
                                    photos = { PhotosTab(graph) },
                                    files = { FilesTab(graph) },
                                )
                            }
                            overlay.Layer()
                        }
                    }
                }
            }
        }
    }
}
