package app.squirrel.ui.search

import app.squirrel.shared.api.ApiError
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class SearchRulesTest {
    @Test
    fun queriesAreTrimmedAndSingleSpaced() {
        assertEquals("dog on the beach", normalizeQuery("  dog \n on   the beach "))
        assertEquals(false, isDocumentQuery(" a "))
        assertEquals(true, isPhotoQuery("a"))
    }

    @Test
    fun aRetryAfterIsWaitedForOnlyWhenShort() {
        assertEquals(2_000L, retryWait(ApiError(429, "rate_limited", "", retryAfterSeconds = 2)))
        assertEquals(RETRY_AFTER_MS, retryWait(ApiError(429, "rate_limited", "")))
        assertNull(retryWait(ApiError(429, "rate_limited", "", retryAfterSeconds = 60)))
    }

    @Test
    fun documentKindsAndPages() {
        assertEquals("JPEG image", documentKind("image/jpeg"))
        assertEquals("Markdown", documentKind("text/markdown"))
        assertNull(documentKind("application/zip"))
        assertEquals("1 page", pagesLabel(1))
        assertNull(pagesLabel(0))
        assertEquals("One-letter words are ignored, and two-letter words only match whole words.", shortWordHint("a to b"))
        assertNull(shortWordHint("invoice"))
    }

    @Test
    fun documentErrorsAreCalm() {
        assertEquals(SEARCH_TOO_FAST, documentSearchError(ApiError(429, "x", "")))
        assertEquals("Searches use 2 to 200 characters.", documentSearchError(ApiError(422, "x", "")))
        assertEquals("Nope", documentSearchError(ApiError(400, "x", "Nope")))
        assertEquals(app.squirrel.ui.photos.UNREACHABLE, documentSearchError(null))
    }
}
