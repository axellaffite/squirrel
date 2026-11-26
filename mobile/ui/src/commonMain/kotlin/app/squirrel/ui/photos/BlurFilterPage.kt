package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.Toggle
import app.squirrel.theme.components.Well
import app.squirrel.ui.shell.PageHeader
import app.squirrel.ui.shell.SystemBack
import io.github.axellaffite.lsdo.compose.RegisterLsDo
import kotlinx.coroutines.launch

/**
 * Photos › Blur filter (from the account menu): whether the photos matching the rules show
 * blurred, the rules themselves (the dynamic album editor's groups, with the live count) and
 * removing the filter. The filter is the account's: the web blurs the same photos.
 */
@Composable
fun BlurFilterPage(api: SquirrelApi, vm: AlbumRulesViewModel, onBack: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    RegisterLsDo("blur_filter", remember(vm) { BlurFilterLsDo(vm, onBack) })
    RegisterLsDo("album_rules", remember(vm) { AlbumRulesLsDo(vm, save = vm::saveFilter, onCancel = onBack) })
    SystemBack(onBack = onBack)
    val filter = state.filter ?: return
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        PageHeader("Blur filter", onBack = onBack, backLabel = "Back to Photos")
        if (filter.loading) return@Column Loading()
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(Tokens.space3),
        ) {
            BasicText(
                "Your photos that match every rule below show blurred, here and on the web, until you choose to show them.",
                style = SquirrelTheme.type.caption,
            )
            filter.loadError?.let { Alert(it, tone = AlertTone.Error) }
            Well {
                Toggle(
                    "Blur matching photos",
                    filter.enabled,
                    onCheckedChange = { on -> scope.launch { vm.setFilterEnabled(on) } },
                    description = if (filter.saved == null) "Applies once the filter is saved" else "Off keeps the rules for later",
                    enabled = !state.saving && !filter.removing,
                )
            }
            filter.notice?.let { Alert(it, tone = AlertTone.Success) }
            PreviewCard(api, state, blurred = true)
            RuleFields(api, state, vm)
            state.error?.let { Alert(it, tone = AlertTone.Error) }
            Button(
                "Save filter",
                onClick = { scope.launch { vm.saveFilter() } },
                enabled = state.problem == null && !state.saving && !filter.removing,
                full = true,
            )
            if (filter.saved != null) {
                Button(
                    "Remove filter",
                    onClick = { scope.launch { vm.removeFilter() } },
                    variant = ButtonVariant.Danger,
                    enabled = !state.saving && !filter.removing,
                    full = true,
                    icon = SquirrelIcon.Trash,
                )
            }
            Spacer(Modifier.height(RailClearance))
        }
    }
}
