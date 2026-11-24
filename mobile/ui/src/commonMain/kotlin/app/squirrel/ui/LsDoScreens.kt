package app.squirrel.ui

import app.squirrel.lsdo.LsDoEntry
import app.squirrel.lsdo.lsdoSpec
import app.squirrel.ui.files.FilesIntent
import app.squirrel.ui.files.FilesState
import app.squirrel.ui.shell.AppIntent
import app.squirrel.ui.shell.AppState

/**
 * Every LS-DO screen of the app, shown or not: the catalog (`scripts/lsdo catalog`,
 * `mobile/lsdo-catalog.json`). A screen added to LS-DO is added here, with how to reach it.
 */
val LsDoScreens: List<LsDoEntry> = listOf(
    LsDoEntry(
        "app",
        "The shell once signed in: the bottom bar's sections (photos, files, shared, backup). Always shown after sign-in.",
        lsdoSpec<AppState, AppIntent>(),
    ),
    LsDoEntry(
        "files",
        "Your own folders, in the Files section. Reached by: app go_to files (it starts at the top level).",
        lsdoSpec<FilesState, FilesIntent>(),
    ),
    LsDoEntry(
        "shared",
        "A folder someone shared with you, in the same browser as files; its top is the shared folder. Reached by: app go_to shared, then tapping a shared folder (the Shared list itself has no LS-DO screen yet).",
        lsdoSpec<FilesState, FilesIntent>(),
    ),
)
