package app.squirrel.ui.search

import app.squirrel.api.models.SnippetPart
import app.squirrel.shared.api.ApiError
import app.squirrel.ui.photos.UNREACHABLE

/** Typing pauses this long before a search is sent (the API allows 20 at once, then 1/s). */
const val SEARCH_DEBOUNCE_MS = 300L

/** After a 429 without Retry-After, the search is retried once after this long. */
const val RETRY_AFTER_MS = 1_000L

/** A Retry-After longer than this is not waited for: the user is told to try again instead. */
const val MAX_RETRY_WAIT_MS = 10_000L

/** While the server's text model starts (503 `busy`), the search is tried again this often… */
const val BUSY_RETRY_MS = 2_000L

/** …this many times, then the user is told to try again. */
const val BUSY_RETRIES = 5

const val MAX_PHOTO_QUERY = 200
const val MAX_PHOTO_RESULTS = 200
const val PHOTO_PAGE = 50

/** Date-ordered pages are larger: two requests lay out the whole result. */
const val PHOTO_DATE_PAGE = 100

const val MIN_DOC_QUERY = 2
const val MAX_DOC_QUERY = 200
const val DOC_PAGE = 20
const val MAX_DOC_RESULTS = 200

/** Snippets are cut after this many characters. */
const val SNIPPET_MAX = 240

const val SEARCH_UNAVAILABLE = "Photo search isn't set up on this server. Ask your administrator to install the search models."
const val SEARCH_BUSY = "Photo search is starting up or busy. Try again in a moment."
const val SEARCH_TOO_FAST = "Searching a little too fast. Trying again in a moment…"
const val STILL_TOO_FAST = "The server is still busy with searches. Wait a moment, then search again."
const val PHOTO_TOO_LONG = "Searches use at most $MAX_PHOTO_QUERY characters."

/** Trimmed, single-spaced. */
fun normalizeQuery(query: String): String = query.trim().replace(Regex("\\s+"), " ")

fun isPhotoQuery(query: String): Boolean = normalizeQuery(query).length in 1..MAX_PHOTO_QUERY

fun isDocumentQuery(query: String): Boolean = normalizeQuery(query).length in MIN_DOC_QUERY..MAX_DOC_QUERY

/** How long to wait before retrying a 429, or null when it is too long to wait for. */
fun retryWait(error: ApiError): Long? {
    val wait = error.retryAfterSeconds?.let { it * 1000 } ?: RETRY_AFTER_MS
    return wait.takeIf { it <= MAX_RETRY_WAIT_MS }
}

/** Calm wording for what can go wrong with a document search. */
fun documentSearchError(error: ApiError?): String = when {
    error == null -> UNREACHABLE
    error.status == 429 -> SEARCH_TOO_FAST
    error.status == 503 -> "The search took too long. Try more precise words."
    error.status == 422 -> "Searches use $MIN_DOC_QUERY to $MAX_DOC_QUERY characters."
    error.fromServer -> error.message.orEmpty()
    else -> UNREACHABLE
}

/**
 * A hint about short words, as the server treats them: one-letter words are ignored, two-letter
 * words only match whole words. Null when none.
 */
fun shortWordHint(query: String): String? {
    val words = normalizeQuery(query).split(' ').filter { it.isNotEmpty() }
    val one = words.any { it.length == 1 }
    val two = words.any { it.length == 2 }
    return when {
        one && two -> "One-letter words are ignored, and two-letter words only match whole words."
        one -> "One-letter words are ignored."
        two -> "Two-letter words only match whole words."
        else -> null
    }
}

/**
 * Snippet parts as the server split them, neighbours of the same kind merged and the whole cut
 * at [max] characters (ending with "…"). Shown as text, never markup.
 */
fun snippetParts(parts: List<SnippetPart>, max: Int = SNIPPET_MAX): List<SnippetPart> {
    val out = mutableListOf<SnippetPart>()
    var length = 0
    for (part in parts) {
        if (length >= max) break
        val room = max - length
        val cut = part.text.length > room
        val text = if (cut) part.text.take(room).trimEnd() + "…" else part.text
        length += if (cut) room else part.text.length
        val last = out.lastOrNull()
        if (last != null && last.highlight == part.highlight) out[out.lastIndex] = last.copy(text = last.text + text)
        else if (text.isNotEmpty()) out += SnippetPart(highlight = part.highlight, text = text)
    }
    return out
}

/** The kind shown in a result's metadata, from its MIME type ("PDF", "Text", "JPEG image"). */
fun documentKind(mime: String?): String? = when {
    mime == null -> null
    mime == "application/pdf" -> "PDF"
    mime == "text/plain" -> "Text"
    mime == "text/markdown" -> "Markdown"
    mime.startsWith("image/") -> mime.removePrefix("image/").uppercase() + " image"
    else -> null
}

/** "3 pages" / "1 page", or null. */
fun pagesLabel(count: Int?): String? = when {
    count == null || count <= 0 -> null
    count == 1 -> "1 page"
    else -> "$count pages"
}
