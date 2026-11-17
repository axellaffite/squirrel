package app.squirrel.ui

import androidx.compose.ui.window.ComposeUIViewController
import app.squirrel.ui.ios.IosGraph
import app.squirrel.ui.ios.IosPhotoActions
import platform.UIKit.UIViewController

/** The Compose UI hosted by the Swift entry point. */
fun MainViewController(): UIViewController = ComposeUIViewController { App(IosGraph.instance, IosPhotoActions) }
