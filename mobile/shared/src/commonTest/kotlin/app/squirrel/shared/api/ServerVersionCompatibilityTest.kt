package app.squirrel.shared.api

import app.squirrel.api.models.AccessLevel
import app.squirrel.api.models.AssetDetails
import app.squirrel.api.models.SyncCheckRequest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * The app and the server are updated separately: an installed app meets newer servers (fields it
 * does not know) and a newer app meets older servers (lists they do not send yet).
 */
class ServerVersionCompatibilityTest {
    private val summary =
        """{"id":"p1","kind":"photo","name":"a.jpg","taken_at":"2024-07-14T18:30:05","thumbnail":{"state":"ready","version":1}}"""

    @Test
    fun fieldsFromANewerServerAreIgnored() {
        val details = ApiJson.decodeFromString<AssetDetails>(
            """{"asset":$summary,"size":10,"albums":[],"tags":["cat"],"people":[],"aesthetic_score":0.7,"faces":[{"box":[1,2,3,4]}]}""",
        )
        assertEquals(listOf("cat"), details.tags)
    }

    @Test
    fun listsAnOlderServerDoesNotSendAreEmpty() {
        // Before Phase 5: no tags, no people.
        val details = ApiJson.decodeFromString<AssetDetails>("""{"asset":$summary,"size":10,"albums":[]}""")
        assertEquals(10L, details.propertySize)
        assertTrue(details.tags.isEmpty())
        assertTrue(details.people.isEmpty())
    }

    @Test
    fun accessAnOlderServerDoesNotSendIsOwner() {
        // Before sharing: everything a server returns is the caller's own.
        val details = ApiJson.decodeFromString<AssetDetails>("""{"asset":$summary,"size":10,"albums":[]}""")
        assertEquals(AccessLevel.owner, details.access)
    }

    @Test
    fun anEmptyRequiredListIsStillSent() {
        val body = ApiJson.encodeToString(SyncCheckRequest(items = emptyList()))
        assertEquals("""{"items":[]}""", body)
    }
}
