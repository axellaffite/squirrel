package app.squirrel.ui.friends

import app.squirrel.api.models.FriendDto
import app.squirrel.api.models.MemberDto
import app.squirrel.api.models.ShareDto
import app.squirrel.api.models.SharePermission
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.time.Instant

class FriendsLogicTest {
    private val at = Instant.parse("2026-11-20T17:05:00Z")

    private fun friend(user: String, name: String = displayName(user)) = FriendDto(username = user, displayName = name, addedAt = at)

    private fun share(user: String) = ShareDto(id = "s-$user", permission = SharePermission.read, createdAt = at, person = MemberDto(username = user, displayName = displayName(user)))

    @Test
    fun aFriendGoesInOnceByDisplayNameIgnoringCase() {
        val list = listOf(friend("anna"), friend("cleo"))
        assertEquals(listOf("anna", "bob", "cleo"), withFriend(list, friend("bob", "bob")).map { it.username })
        assertEquals(listOf("anna", "cleo"), withFriend(list, friend("cleo")).map { it.username })
        assertEquals("Cleo B.", withFriend(list, friend("cleo", "Cleo B.")).last().displayName)
        // Same display name: by username, as the server orders them.
        assertEquals(listOf("anna", "zed", "zoe"), withFriend(listOf(friend("anna"), friend("zoe", "Z")), friend("zed", "Z")).map { it.username })
    }

    @Test
    fun picksAreTheFriendsWithoutAccessOnceTheSharesAreKnown() {
        val friends = listOf(friend("anna"), friend("bob"), friend("cleo"))
        assertEquals(listOf("bob", "cleo"), friendPicks(friends, listOf(share("anna"))).map { it.username })
        assertEquals(emptyList(), friendPicks(friends, null))
        assertEquals(emptyList(), friendPicks(emptyList(), emptyList()))
    }
}
