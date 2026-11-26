package app.squirrel.ui

import app.squirrel.ui.backup.AlbumPickerIntent
import app.squirrel.ui.backup.AlbumPickerView
import app.squirrel.ui.backup.BackupIntent
import app.squirrel.ui.backup.BackupSettingsIntent
import app.squirrel.ui.backup.BackupSettingsView
import app.squirrel.ui.backup.BackupView
import app.squirrel.ui.explore.ExploreIntent
import app.squirrel.ui.explore.ExploreView
import app.squirrel.ui.files.FilesIntent
import app.squirrel.ui.files.FilesNavIntent
import app.squirrel.ui.files.FilesNavState
import app.squirrel.ui.files.FilesState
import app.squirrel.ui.files.UploadsIntent
import app.squirrel.ui.files.UploadsState
import app.squirrel.ui.notifications.NotificationsIntent
import app.squirrel.ui.notifications.NotificationsView
import app.squirrel.ui.photos.AlbumIntent
import app.squirrel.ui.photos.AlbumListView
import app.squirrel.ui.photos.AlbumView
import app.squirrel.ui.photos.AlbumsIntent
import app.squirrel.ui.photos.PhotosIntent
import app.squirrel.ui.photos.PhotosView
import app.squirrel.ui.photos.TimelineIntent
import app.squirrel.ui.photos.TimelineView
import app.squirrel.ui.search.DocumentSearchIntent
import app.squirrel.ui.search.DocumentSearchLsDoState
import app.squirrel.ui.search.PhotoSearchIntent
import app.squirrel.ui.search.PhotoSearchView
import app.squirrel.ui.friends.FriendsIntent
import app.squirrel.ui.friends.FriendsView
import app.squirrel.ui.sharing.ShareIntent
import app.squirrel.ui.sharing.ShareView
import app.squirrel.ui.sharing.SharedListIntent
import app.squirrel.ui.sharing.SharedListState
import app.squirrel.ui.sharing.SharedNavIntent
import app.squirrel.ui.sharing.SharedNavState
import app.squirrel.ui.shell.AppIntent
import app.squirrel.ui.shell.AppState
import app.squirrel.ui.signin.SignInIntent
import app.squirrel.ui.signin.SignInView
import io.github.axellaffite.lsdo.LsDoEntry
import io.github.axellaffite.lsdo.LsDoSpec
import io.github.axellaffite.lsdo.lsdoSpec

/**
 * Every LS-DO screen of the app, shown or not: the catalog (`scripts/lsdo catalog`,
 * `mobile/lsdo-catalog.json`). A screen added to LS-DO is added here, with how to reach it.
 */
private fun entry(name: String, description: String, spec: LsDoSpec<*, *>) = LsDoEntry(name, description, spec)

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
        "A folder someone shared with you, in the same browser as files; its top is the shared folder. Reached by: app go_to shared, then shared_list open <share id of a folder>.",
        lsdoSpec<FilesState, FilesIntent>(),
    ),
    entry("notifications", "The bell in the top row (unread count) and its sheet: open, page, mark read, open an item (its subject in the Shared section: shared_nav, shared_list). Always shown after sign-in.", lsdoSpec<NotificationsView, NotificationsIntent>()),
    entry("signin", "The sign-in flow: the server address, then username and password (step: server / credentials / signed_in). The password is never in the state (passwordSet only). Reached by: a fresh install or after sign-out.", lsdoSpec<SignInView, SignInIntent>()),
    entry("files_nav", "The Files section's pages (browse, search): search opens document search, back closes it. Reached by: app go_to files.", lsdoSpec<FilesNavState, FilesNavIntent>()),
    entry("document_search", "Search of file names and PDF/text contents; snippets mark matches with **...**. Reached by: app go_to files, then files_nav search.", lsdoSpec<DocumentSearchLsDoState, DocumentSearchIntent>()),
    entry("uploads", "Uploads from this phone with their progress and errors (a do never waits for them: ls again). Reached by: any folder page (files or shared).", lsdoSpec<UploadsState, UploadsIntent>()),
    entry("shared_nav", "The Shared section's pages (list, folder, album, photo, friends): friends opens Friends from the list, back returns to the list. Reached by: app go_to shared.", lsdoSpec<SharedNavState, SharedNavIntent>()),
    entry("shared_list", "What others shared with you (open a folder, which shows the shared browser, an album or a photo; leave a share), and what you shared with others (open_mine; manage opens its Share sheet). Reached by: app go_to shared.", lsdoSpec<SharedListState, SharedListIntent>()),
    entry("friends", "Your friends (a one-way list: they are not told): add by username or from suggestions (people you share with), remove. Friends without access show as one-tap picks in the Share sheet. Reached by: app go_to shared, then shared_nav friends.", lsdoSpec<FriendsView, FriendsIntent>()),
    entry("share", "The Share sheet of a folder, file, album or photo you own: share by username or with a friend's pick (share_with), change or stop a share, close. Reached by: tapping Share on something you own (a folder page's header or a row's actions, an album's header, the photo viewer); no LS-DO action opens it yet (scripts/lsdo ui shows the button).", lsdoSpec<ShareView, ShareIntent>()),
    entry("photos", "The Photos section's navigation: show timeline / albums / explore, open search, back. Reached by: app go_to photos.", lsdoSpec<PhotosView, PhotosIntent>()),
    entry("timeline", "Photos by month: page, open a photo, add photos to an album; also the filtered timeline opened from explore. Reached by: app go_to photos (the default), or explore open <id>.", lsdoSpec<TimelineView, TimelineIntent>()),
    entry("albums", "The album list: open, create. Reached by: app go_to photos, then photos show albums.", lsdoSpec<AlbumListView, AlbumsIntent>()),
    entry("album", "One album's page: its photos, rename, cover and removal (owner or write access). Reached by: albums open <id> or albums create <name>.", lsdoSpec<AlbumView, AlbumIntent>()),
    entry("explore", "People, places, tags and cameras: open a card (a filtered timeline), see all of a section. Reached by: app go_to photos, then photos show explore.", lsdoSpec<ExploreView, ExploreIntent>()),
    entry("photo_search", "Photo search: text, filter, order, open a result. Reached by: app go_to photos, then photos search.", lsdoSpec<PhotoSearchView, PhotoSearchIntent>()),
    entry("backup", "Backup status, progress, counts and failures: back up now, retry failures, open settings. Needs photo access (else only refresh). Reached by: app go_to backup.", lsdoSpec<BackupView, BackupIntent>()),
    entry("backup_settings", "Backup settings: the four toggles and this phone's name; back returns to backup. Reached by: backup open_settings.", lsdoSpec<BackupSettingsView, BackupSettingsIntent>()),
    entry("album_picker", "Which albums (iOS) or folders (Android) are backed up: open the sheet, toggle, select all. Reached by: app go_to backup (also on backup_settings).", lsdoSpec<AlbumPickerView, AlbumPickerIntent>()),
)
