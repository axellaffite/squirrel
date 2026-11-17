package app.squirrel.ui.photos

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.AssetSummary
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.AlertTone
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonSize
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.Loading

/** Bottom space so the last row clears the floating rail. */
val RailClearance = Tokens.railBarH + Tokens.frame * 2

/** The timeline: month headings, justified rows, placeholder rows until each month loads. */
@Composable
fun TimelineList(api: SquirrelApi, vm: PhotosViewModel, onOpen: (AssetSummary) -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    when {
        state.loadingMonths -> return Loading()
        state.months.isEmpty() && state.error != null -> {
            ErrorRetry(state.error!!, vm::retry)
            return
        }
        state.empty -> {
            EmptyState(SquirrelIcon.Images, "No photos yet", "Backed-up photos appear here.")
            return
        }
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val width = maxWidth.value.toInt()
        val blocks = remember(state.months, width) { timelineBlocks(state.months, width) }
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(Tokens.space1),
            contentPadding = PaddingValues(bottom = RailClearance),
        ) {
            state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::retry) } }
            items(blocks, key = { it.key }, contentType = { it::class }) { block ->
                when (block) {
                    is TimelineBlock.Header -> {
                        LaunchedEffect(block.month) { vm.ensure(block.month) }
                        BasicText(
                            block.label,
                            style = SquirrelTheme.type.cardTitle,
                            modifier = Modifier
                                .height(Tokens.photoMonthHead)
                                .wrapContentHeight(Alignment.Bottom)
                                .padding(bottom = Tokens.space2)
                                .semantics { heading() },
                        )
                    }
                    is TimelineBlock.Row -> PhotoRow(api, block.height, block.tiles, onOpen)
                    is TimelineBlock.Placeholder -> {
                        LaunchedEffect(block.month) { vm.ensure(block.month) }
                        PlaceholderRow(block.height)
                    }
                }
            }
        }
    }
}

@Composable
fun ErrorRetry(message: String, onRetry: () -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Tokens.space2)) {
        Alert(message, tone = AlertTone.Error) {
            Button("Try again", onClick = onRetry, variant = ButtonVariant.Secondary, size = ButtonSize.Sm)
        }
    }
}

