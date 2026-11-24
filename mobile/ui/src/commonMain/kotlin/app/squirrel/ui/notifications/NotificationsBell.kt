package app.squirrel.ui.notifications

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.squirrel.api.models.NotificationDto
import app.squirrel.shared.notifications.actorName
import app.squirrel.shared.notifications.notificationAction
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.IconButton
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.Sheet
import app.squirrel.theme.tabular
import app.squirrel.ui.AppGraph
import app.squirrel.ui.OnForeground
import app.squirrel.ui.RegisterLsDo
import app.squirrel.ui.sharing.SharedRoute
import app.squirrel.ui.sharing.commentWhen
import app.squirrel.ui.shell.AppTab
import app.squirrel.ui.shell.Navigator
import kotlin.time.Clock

/** Asks the system for permission to post notifications (Android 13+, iOS); the first time the inbox opens. */
fun interface NotificationPrompt {
    fun request()
}

/**
 * The bell in the shell's top row, with the unread count, and its sheet. A notification opens
 * its subject in the Shared section ([select] switches to it).
 */
@Composable
fun NotificationsBell(graph: AppGraph, prompt: NotificationPrompt?, select: (AppTab) -> Unit) {
    val api = graph.api ?: return
    val vm = viewModel(key = "notifications") { NotificationsViewModel(api) }
    // The Shared section's own stack: opening a subject pushes its page there.
    val nav = viewModel(key = "nav-shared") { Navigator<SharedRoute>(SharedRoute.List) }
    val state by vm.state.collectAsStateWithLifecycle()
    OnForeground { vm.resumed() }
    val go: (NotificationTarget) -> Unit = remember(nav, select) {
        { target ->
            val route = when (target) {
                is NotificationTarget.Folder -> SharedRoute.Folder(target.id)
                is NotificationTarget.Album -> SharedRoute.Album(target.id, target.name)
                is NotificationTarget.Photo -> SharedRoute.Photo(target.id, target.name)
                is NotificationTarget.FileNode -> SharedRoute.File(target.node)
                // Never handed over: the view model fetches a file's details first (FileNode).
                is NotificationTarget.File -> null
            }
            if (route != null) {
                select(AppTab.Shared)
                nav.reset()
                nav.push(route)
            }
        }
    }
    RegisterLsDo("notifications", remember(vm, go) { NotificationsLsDo(vm, go) })

    val badge = unreadBadge(state.unread)
    Box {
        IconButton(
            SquirrelIcon.Bell,
            label = if (badge == null) "Notifications" else "Notifications, ${state.unread} unread",
            onClick = {
                vm.open()
                prompt?.let { graph.askNotificationsOnce(it::request) }
            },
        )
        if (badge != null) {
            val scheme = SquirrelTheme.scheme
            BasicText(
                badge,
                style = SquirrelTheme.type.badge.copy(color = scheme.onAccent).tabular(),
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .clearAndSetSemantics {}
                    .background(scheme.accent, Shapes.pill)
                    .padding(horizontal = Tokens.space1),
            )
        }
    }
    if (state.open) NotificationsSheet(state, vm) { id -> vm.select(id, go) }
}

@Composable
private fun NotificationsSheet(state: NotificationsState, vm: NotificationsViewModel, onSelect: (String) -> Unit) {
    Sheet("Notifications", onDismiss = vm::close) {
        if (state.unread > 0) {
            Button("Mark all as read", onClick = vm::markAllRead, variant = ButtonVariant.Quiet, size = ButtonSize.Sm, icon = SquirrelIcon.Check)
        }
        state.error?.let { Alert(it, tone = AlertTone.Error) }
        val items = state.items
        when {
            items == null -> if (state.error == null) Loading()
            items.isEmpty() -> EmptyState(SquirrelIcon.Bell, "No notifications", "When someone shares something with you or comments on it, it shows here.")
            else -> {
                val now = Clock.System.now()
                items.forEach { item -> NotificationItem(item, commentWhen(item.createdAt, now)) { onSelect(item.id) } }
            }
        }
        if (state.cursor != null) {
            Button("Show more", onClick = vm::more, variant = ButtonVariant.Quiet, size = ButtonSize.Sm, enabled = !state.loadingMore)
        }
    }
}

@Composable
private fun NotificationItem(item: NotificationDto, whenText: String, onClick: () -> Unit) {
    val type = SquirrelTheme.type
    val scheme = SquirrelTheme.scheme
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { if (!item.read) stateDescription = "Unread" }
            .padding(vertical = Tokens.space2),
        horizontalArrangement = Arrangement.spacedBy(Tokens.space2),
        verticalAlignment = Alignment.Top,
    ) {
        // The unread dot, or the same room so the lines stay aligned.
        Box(Modifier.padding(top = Tokens.space2).size(Tokens.locatorDot).then(if (item.read) Modifier else Modifier.background(scheme.accent, Shapes.circle)))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Tokens.space1)) {
            BasicText(
                buildAnnotatedString {
                    withStyle(type.label.toSpanStyle()) { append(actorName(item)) }
                    append(" ")
                    append(notificationAction(item))
                },
                style = type.body,
            )
            BasicText(whenText, style = type.caption.tabular())
        }
    }
}
