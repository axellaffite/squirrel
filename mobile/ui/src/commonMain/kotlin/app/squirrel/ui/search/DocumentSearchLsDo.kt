package app.squirrel.ui.search

import app.squirrel.api.models.DocumentHit
import io.github.axellaffite.lsdo.Doc
import io.github.axellaffite.lsdo.LsDoScreen
import io.github.axellaffite.lsdo.lsdoSpec
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class DocumentResult(
    @Doc("The file's id") val id: String,
    val name: String,
    @Doc("Its folder's id") val folderId: String?,
    val mime: String?,
    @Doc("Kind, pages and date, as the row shows them") val meta: String,
    @Doc("The passage around the matches, matched words in **…**; empty when only the name matched") val snippet: String,
)

@Serializable
data class DocumentSearchLsDoState(
    @Doc("What is typed in the search field") val typed: String = "",
    @Doc("The query the results belong to; empty before any search") val query: String = "",
    val results: List<DocumentResult> = emptyList(),
    @Doc("More results wait: more loads them") val hasMore: Boolean = false,
    @Doc("The results are for what is typed and the search is done: empty results mean nothing matched") val settled: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
    @Doc("About short words in what is typed, as the field shows it") val hint: String? = null,
)

/** What an agent can do in document search (LS-DO); each maps to the call the search field makes. */
@Serializable
sealed interface DocumentSearchIntent {
    @Serializable @SerialName("type")
    @Doc("Type in the search field, replacing what is there: the search starts after a short pause. At least $MIN_DOC_QUERY characters search")
    data class Type(val text: String) : DocumentSearchIntent

    @Serializable @SerialName("search") @Doc("Search now for what is typed, as the keyboard's search key does. Only with $MIN_DOC_QUERY characters or more typed")
    data object Search : DocumentSearchIntent

    @Serializable @SerialName("more") @Doc("Load more results, as the \"Load more\" button does. Only when hasMore")
    data object More : DocumentSearchIntent

    @Serializable @SerialName("retry") @Doc("Try the failed search again, after an error")
    data object Retry : DocumentSearchIntent
}

/** The search as an agent reads it: names, places and snippets as text. */
internal fun documentSearchLsDoState(state: DocumentSearchState): DocumentSearchLsDoState = DocumentSearchLsDoState(
    typed = state.typed,
    query = state.query,
    results = state.items.map(::documentResult),
    hasMore = state.hasMore,
    settled = state.settled,
    loading = state.loading,
    error = state.error,
    hint = state.hint,
)

private fun documentResult(hit: DocumentHit) = DocumentResult(
    id = hit.node.id,
    name = hit.node.name,
    folderId = hit.node.parentId,
    mime = hit.node.mime,
    meta = documentMeta(hit),
    snippet = snippetParts(hit.snippet).joinToString("") { if (it.highlight) "**${it.text}**" else it.text },
)

/**
 * Document search as an LS-DO screen ("document_search"). Opening a result is left out: it
 * hands the file to another app, which an agent cannot see.
 */
class DocumentSearchLsDo(private val vm: DocumentSearchViewModel) : LsDoScreen<DocumentSearchLsDoState, DocumentSearchIntent> {
    override val lsdo = lsdoSpec<DocumentSearchLsDoState, DocumentSearchIntent>()
    override val state: StateFlow<DocumentSearchLsDoState> = vm.listing

    override suspend fun perform(intent: DocumentSearchIntent): String? {
        when (intent) {
            is DocumentSearchIntent.Type -> vm.type(intent.text)
            DocumentSearchIntent.Search ->
                if (!isDocumentQuery(vm.state.value.typed)) return "Type $MIN_DOC_QUERY to $MAX_DOC_QUERY characters to search." else vm.now()
            DocumentSearchIntent.More -> vm.more()
            DocumentSearchIntent.Retry -> vm.retry()
        }
        return null
    }

    override fun availableIntents(state: DocumentSearchLsDoState): Set<String> = buildSet {
        add("type")
        if (isDocumentQuery(state.typed)) add("search")
        if (state.hasMore && !state.loading) add("more")
        if (state.error != null) add("retry")
    }

    override fun isBusy(state: DocumentSearchLsDoState): Boolean = state.loading
}
