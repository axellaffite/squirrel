package app.squirrel.ui.friends

import app.squirrel.api.models.FriendDto
import app.squirrel.api.models.ShareDto

/** The server's order: by display name ignoring case, then username. */
private val BY_NAME = compareBy<FriendDto>({ it.displayName.lowercase() }, { it.username })

/** [friends] with [friend] in it once (a newer copy replaces the old one), in the server's order. */
fun withFriend(friends: List<FriendDto>, friend: FriendDto): List<FriendDto> =
    (friends.filterNot { it.username == friend.username } + friend).sortedWith(BY_NAME)

/**
 * The Share sheet's one-tap picks: friends who do not have access yet. None until the shares are
 * known, so a pick never offers someone who already has access.
 */
fun friendPicks(friends: List<FriendDto>, shares: List<ShareDto>?): List<FriendDto> {
    if (shares == null) return emptyList()
    val shared = shares.map { it.person.username }.toSet()
    return friends.filterNot { it.username in shared }
}
