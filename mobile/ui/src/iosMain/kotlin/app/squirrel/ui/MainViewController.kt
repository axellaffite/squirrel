package app.squirrel.ui

import androidx.compose.ui.window.ComposeUIViewController
import app.squirrel.ui.ios.IosFilePicker
import app.squirrel.ui.ios.IosGraph
import app.squirrel.ui.ios.IosPhotoActions
import platform.UIKit.UIViewController

/** One picker for the process: it keeps the delegate of the sheet on screen. */
private val picker = IosFilePicker()

/** The Compose UI hosted by the Swift entry point. */
fun MainViewController(): UIViewController = ComposeUIViewController { App(IosGraph.instance, IosPhotoActions, picker) }
