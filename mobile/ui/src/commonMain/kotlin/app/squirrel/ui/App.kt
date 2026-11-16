package app.squirrel.ui

import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import app.squirrel.theme.SquirrelTheme

/** The app's root composable, shared by Android and iOS. */
@Composable
fun App() {
    SquirrelTheme {
        BasicText("Squirrel")
    }
}
