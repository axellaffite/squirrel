package app.squirrel.ui.search

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.squirrel.api.models.DocumentHit
import app.squirrel.api.models.SnippetPart
import app.squirrel.shared.api.SquirrelApi
import app.squirrel.theme.Shapes
import app.squirrel.theme.SquirrelIcon
import app.squirrel.theme.SquirrelTheme
import app.squirrel.theme.Tokens
import app.squirrel.theme.cardShadow
import app.squirrel.theme.components.Alert
import app.squirrel.theme.components.Button
import app.squirrel.theme.components.ButtonVariant
import app.squirrel.theme.components.EmptyState
import app.squirrel.theme.components.IconWell
import app.squirrel.theme.components.Loading
import app.squirrel.theme.components.MinTouchTarget
import app.squirrel.theme.components.Progress
import app.squirrel.theme.tabular
import app.squirrel.ui.files.OpenerConfirm
import app.squirrel.ui.files.fileIcon
import app.squirrel.ui.files.shortDate
import app.squirrel.ui.photos.ErrorRetry
import app.squirrel.ui.photos.RailClearance

/** "PDF · 3 pages · 12 March 2026". */
fun documentMeta(hit: DocumentHit): String =
    listOfNotNull(documentKind(hit.node.mime), pagesLabel(hit.pageCount), shortDate(hit.node.modifiedAt)).joinToString(" · ")

/** Matched words as `--accent-soft` highlights with `--ink` text (MASTER "Search field"). */
@Composable
private fun snippetText(parts: List<SnippetPart>): AnnotatedString {
    val scheme = SquirrelTheme.scheme
    return buildAnnotatedString {
        snippetParts(parts).forEach { part ->
            if (part.highlight) withStyle(SpanStyle(background = scheme.accentSoft, color = scheme.ink)) { append(part.text) } else append(part.text)
        }
    }
}

@Composable
internal fun DocumentResults(api: SquirrelApi, vm: DocumentSearchViewModel, state: DocumentSearchState) {
    val opener by vm.opener.state.collectAsStateWithLifecycle()
    val count = if (state.settled) "${state.items.size}${if (state.hasMore) " or more" else ""} results" else ""
    BasicText("", modifier = Modifier.semantics { contentDescription = count; liveRegion = LiveRegionMode.Polite })
    opener.message?.let { Alert(it) }
    if (state.error != null && state.items.isEmpty()) ErrorRetry(state.error, vm::retry)
    when {
        state.items.isNotEmpty() -> LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(Tokens.space2),
            contentPadding = PaddingValues(bottom = RailClearance),
        ) {
            items(state.items, key = { it.node.id }) { hit -> DocumentRow(hit, opener.downloads[hit.node.id]) { vm.opener.open(hit.node) } }
            state.error?.let { error -> item(key = "error") { ErrorRetry(error, vm::retry) } }
            if (state.hasMore) {
                item(key = "more") {
                    Button("Load more", onClick = vm::more, variant = ButtonVariant.Secondary, enabled = !state.loading, full = true)
                }
            }
        }
        state.settled && state.error == null -> EmptyState(
            SquirrelIcon.Search,
            "Nothing found",
            "Search looks at file names, PDFs and text files. Scanned documents without text are not searchable yet.",
        )
        state.loading -> Loading(text = "Searching…")
        else -> EmptyState(SquirrelIcon.FileText, "Find your files", "Search file names, and PDFs and text files by their content.")
    }
    OpenerConfirm(vm.opener)
}

/** A row card with the file's icon, name, metadata and its highlighted snippet. */
@Composable
private fun DocumentRow(hit: DocumentHit, progress: Float?, onOpen: () -> Unit) {
    val type = SquirrelTheme.type
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .cardShadow(Shapes.card)
            .background(SquirrelTheme.scheme.card, Shapes.card)
            .clickable(role = Role.Button, onClickLabel = "Open", onClick = onOpen)
            .heightIn(min = MinTouchTarget)
            .padding(horizontal = Tokens.space3, vertical = Tokens.space2),
        verticalArrangement = Arrangement.spacedBy(Tokens.space1),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(Tokens.space3), verticalAlignment = Alignment.CenterVertically) {
            IconWell(fileIcon(hit.node.mime))
            Column(Modifier.weight(1f)) {
                BasicText(hit.node.name, style = type.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                BasicText(documentMeta(hit), style = type.caption.tabular(), maxLines = 1)
            }
            if (progress != null) Box(Modifier.width(Tokens.space7)) { Progress(progress, label = "Downloading ${hit.node.name}") }
        }
        if (hit.snippet.isNotEmpty()) BasicText(snippetText(hit.snippet), style = type.caption.copy(color = SquirrelTheme.scheme.ink))
    }
}
