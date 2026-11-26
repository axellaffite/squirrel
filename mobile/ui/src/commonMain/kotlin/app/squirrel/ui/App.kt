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
import app.squirrel.theme.blurredUnderModals
import app.squirrel.theme.components.ground
import app.squirrel.ui.backup.BackupTab
import app.squirrel.ui.images.LocalImageLoader
import app.squirrel.ui.photos.BlurView
import app.squirrel.ui.photos.BlurredLsDo
import io.github.axellaffite.lsdo.compose.RegisterLsDo
import app.squirrel.ui.photos.LocalBlur
import app.squirrel.ui.photos.PhotosRoute
import app.squirrel.ui.photos.PhotosTab
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.ui.shell.Navigator
import app.squirrel.ui.files.FilePicker
import app.squirrel.ui.files.FilesTab
import app.squirrel.ui.files.addMenu
import app.squirrel.ui.files.LocalFilePicker
import app.squirrel.ui.sharing.SharedTab
import app.squirrel.ui.notifications.NotificationPrompt
import app.squirrel.ui.notifications.NotificationsBell
import app.squirrel.ui.backup.PhotoAccessActions
import app.squirrel.ui.shell.AppTab
import app.squirrel.ui.shell.Shell
import app.squirrel.ui.signin.SignInFlow

/** The app's root composable, shared by Android and iOS. */
@Composable
fun App(
    graph: AppGraph,
    actions: PhotoAccessActions = PhotoAccessActions.None,
    picker: FilePicker? = null,
    notifications: NotificationPrompt? = null,
) {
    SquirrelTheme {
        val state by graph.state.collectAsStateWithLifecycle()
        val images by graph.imageLoader.collectAsStateWithLifecycle()
        val blurState by graph.blurred.state.collectAsStateWithLifecycle()
        val blur = remember(blurState) {
            BlurView(blurState, graph.blurred::reveal, graph.blurred::setRevealAll)
        }
        CompositionLocalProvider(LocalImageLoader provides images, LocalFilePicker provides picker, LocalBlur provides blur) {
            // Sheets and dialogs open over this content blurred (MASTER.md "Dialog / sheet").
            Box(Modifier.fillMaxSize().ground().blurredUnderModals()) {
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
                            // Coming back to the app: the blur filter may have changed elsewhere (web).
                            OnForeground { graph.blurred.refresh() }
                            RegisterLsDo("blurred", remember(graph) { BlurredLsDo(graph.blurred) })
                            // The account menu's "Blur filter" opens on the Photos tab's navigator.
                            val photosNav = viewModel(key = "nav-photos") { Navigator<PhotosRoute>(PhotosRoute.Home) }
                            // Under the full-screen layer, the shell is hidden from assistive tech.
                            val covered = overlay.content != null
                            Box(Modifier.fillMaxSize().then(if (covered) Modifier.clearAndSetSemantics {} else Modifier)) {
                                Shell(
                                    current.account,
                                    onSignOut = graph::signOut,
                                    onRefresh = graph::refreshAccount,
                                    onBlurFilter = { photosNav.push(PhotosRoute.BlurFilter()) },
                                    backup = { BackupTab(graph, actions) },
                                    photos = { PhotosTab(graph) },
                                    files = { FilesTab(graph) },
                                    shared = { SharedTab(graph) },
                                    topActions = { select -> NotificationsBell(graph, notifications, select) },
                                    addMenu = { select -> addMenu(graph) { select(AppTab.Files) } },
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
