package app.squirrel.ui.shell

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.BottomRail
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.MenuAction
import app.squirrel.theme.components.RailItem
import app.squirrel.theme.popShadow
import app.squirrel.theme.tabular
import app.squirrel.ui.AccountInfo
import io.github.axellaffite.lsdo.compose.RegisterLsDo

private enum class Tab(val label: String, val icon: SquirrelIcon, val title: String, val line: String) {
    Photos("Photos", SquirrelIcon.Images, "Photos are not in the app yet", "Your timeline arrives in a later version of the app."),
    Files("Files", SquirrelIcon.Folder, "Files are not in the app yet", "Browsing your folders arrives in a later version."),
    Shared("Shared", SquirrelIcon.Users, "Nothing shared yet", "Folders, files and albums shared with you appear here."),
    Backup("Backup", SquirrelIcon.Upload, "", ""),
}

/**
 * MASTER.md phone shell: top row with the wordmark and account menu, the floating bottom bar with
 * its centre add button, whose menu holds [addMenu]'s actions.
 */
@Composable
fun Shell(
    account: AccountInfo,
    onSignOut: () -> Unit,
    onRefresh: () -> Unit,
    /** Opens the blur filter's settings (on the Photos tab); null hides the entry. */
    onBlurFilter: (() -> Unit)? = null,
    backup: @Composable () -> Unit = {},
    photos: (@Composable () -> Unit)? = null,
    files: (@Composable () -> Unit)? = null,
    shared: (@Composable () -> Unit)? = null,
    /** Global actions before the account menu (the notifications bell); [select] shows a section. */
    topActions: @Composable (select: (AppTab) -> Unit) -> Unit = {},
    /**
     * The add menu's actions (and the sheets they open, composed here); [select] shows a section.
     * Always composed, so a sheet it shows outlives the bar hiding for the keyboard.
     */
    addMenu: @Composable (select: (AppTab) -> Unit) -> List<MenuAction> = { emptyList() },
) {
    var tab by rememberSaveable { mutableStateOf(Tab.Photos.name) }
    var menuOpen by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(Unit) { onRefresh() }
    val current = Tab.valueOf(tab)
    // LS-DO: an agent switches sections through the same state the bottom bar sets.
    val lsdo = remember { ShellLsDo(AppTab.valueOf(current.name)) { tab = it.name } }
    LaunchedEffect(current) { lsdo.shown(AppTab.valueOf(current.name)) }
    RegisterLsDo("app", lsdo)
    Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
        Column(Modifier.fillMaxSize().padding(horizontal = Tokens.gutter)) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(vertical = Tokens.space2),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                BasicText("Squirrel", style = SquirrelTheme.type.wordmark, modifier = Modifier.weight(1f))
                topActions { tab = it.name }
                Box {
                    IconButton(SquirrelIcon.User, label = "Account menu", onClick = {
                        menuOpen = !menuOpen
                        if (menuOpen) onRefresh()
                    })
                    if (menuOpen) {
                        Popup(
                            alignment = Alignment.TopEnd,
                            onDismissRequest = { menuOpen = false },
                            properties = PopupProperties(focusable = true),
                        ) {
                            AccountMenu(
                                account,
                                onSignOut = {
                                    menuOpen = false
                                    onSignOut()
                                },
                                onBlurFilter = onBlurFilter?.let { open ->
                                    {
                                        menuOpen = false
                                        tab = Tab.Photos.name
                                        open()
                                    }
                                },
                            )
                        }
                    }
                }
            }
            // Each tab draws its own header (a sub-page puts a back button before its title).
            val content = when (current) {
                Tab.Backup -> backup
                Tab.Photos -> photos
                Tab.Files -> files
                Tab.Shared -> shared
            }
            if (content != null) {
                content()
            } else {
                PageHeader(current.label)
                EmptyState(current.icon, current.title, current.line)
            }
        }
        val addActions = addMenu { tab = it.name }
        // While the keyboard is up, the bar would ride above it and cover the field being typed in.
        // It floats `--space-3` above the bottom inset, `--frame` from the sides.
        if (WindowInsets.ime.getBottom(LocalDensity.current) == 0) BottomRail(
            items = Tab.entries.map { RailItem(it.name, it.label, it.icon) },
            selected = tab,
            onSelect = { tab = it },
            addLabel = "Add to Squirrel",
            addActions = addActions,
            modifier = Modifier.align(Alignment.BottomCenter).padding(horizontal = Tokens.frame).padding(bottom = Tokens.space3),
        )
    }
}

@Composable
private fun AccountMenu(account: AccountInfo, onSignOut: () -> Unit, onBlurFilter: (() -> Unit)?) {
    val type = SquirrelTheme.type
    Column(
        modifier = Modifier
            .padding(top = Tokens.tap)
            .width(Tokens.menuW)
            .popShadow(Shapes.card)
            .background(SquirrelTheme.scheme.card, Shapes.card)
            .padding(Tokens.cardPad),
        verticalArrangement = Arrangement.spacedBy(Tokens.space1),
    ) {
        BasicText(account.displayName, style = type.label)
        if (account.username.isNotEmpty()) BasicText(account.username, style = type.caption)
        BasicText(account.serverUrl, style = type.caption.tabular(), maxLines = 1, overflow = TextOverflow.Ellipsis)
        BasicText("This phone: ${account.deviceName}", style = type.caption)
        if (onBlurFilter != null) {
            Button("Blur filter", onClick = onBlurFilter, variant = ButtonVariant.Secondary, full = true, icon = SquirrelIcon.EyeOff)
        }
        Button("Sign out", onClick = onSignOut, variant = ButtonVariant.Secondary, full = true, icon = SquirrelIcon.LogOut)
    }
}
