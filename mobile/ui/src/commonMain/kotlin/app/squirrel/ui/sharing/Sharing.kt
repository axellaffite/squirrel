package app.squirrel.ui.sharing

import app.squirrel.api.models.AccessLevel
import app.squirrel.api.models.CommentDto
import app.squirrel.api.models.MemberDto
import app.squirrel.api.models.PathSegmentDto
import app.squirrel.api.models.ShareDto
import app.squirrel.api.models.SharePermission
import app.squirrel.ui.files.shortDate
import kotlinx.datetime.TimeZone
import kotlinx.datetime.toLocalDateTime
import kotlin.time.Duration.Companion.days
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Instant

/** What a share or a comment thread is about: one folder or file, or one album. */
sealed interface Subject {
    val id: String

    data class Node(override val id: String) : Subject
    data class Album(override val id: String) : Subject

    val nodeId: String? get() = (this as? Node)?.id
    val albumId: String? get() = (this as? Album)?.id
}

/** The server's comment limit, in characters once trimmed. */
const val COMMENT_MAX = 4000

/** An account's name; comments of deleted accounts keep a `null` author. */
fun memberName(member: MemberDto?): String = member?.displayName ?: "Deleted user"

fun permissionLabel(permission: SharePermission): String = if (permission == SharePermission.write) "Can edit" else "Can view"

/** What the caller may do with a folder, file, album or photo, from its `access`. */
data class Abilities(
    /** Share, rename, set the cover: the subject itself. */
    val owner: Boolean,
    /** Add, rename and move what is inside; add and remove one's own photos in an album. */
    val edit: Boolean,
) {
    companion object {
        /** An unknown level (a newer server) gets the least. */
        fun of(access: AccessLevel): Abilities = Abilities(owner = access == AccessLevel.owner, edit = access == AccessLevel.owner || access == AccessLevel.write)
    }
}

/** A share back from the server: the same id replaces the old one (a permission change). */
fun upsertShare(shares: List<ShareDto>, share: ShareDto): List<ShareDto> =
    if (shares.any { it.id == share.id }) shares.map { if (it.id == share.id) share else it } else shares + share

/** The next page of a thread (oldest first) after the ones shown. */
fun mergeComments(shown: List<CommentDto>, page: List<CommentDto>): List<CommentDto> {
    val seen = shown.map { it.id }.toSet()
    return shown + page.filter { it.id !in seen }
}

fun commentProblem(body: String): String? {
    val length = body.trim().length
    return when {
        length == 0 -> "Write a comment first."
        length > COMMENT_MAX -> "Comments have at most $COMMENT_MAX characters."
        else -> null
    }
}

/** "Just now", "5 min ago", "3 h ago", then a date and time ("12 March 2026, 18:30"). */
fun commentWhen(at: Instant, now: Instant, zone: TimeZone = TimeZone.currentSystemDefault()): String {
    val age = now - at
    return when {
        age < 1.minutes -> "Just now"
        age < 1.hours -> "${age.inWholeMinutes} min ago"
        age < 1.days -> "${age.inWholeHours} h ago"
        else -> {
            val time = at.toLocalDateTime(zone).time
            "${shortDate(at, zone)}, ${time.hour.toString().padStart(2, '0')}:${time.minute.toString().padStart(2, '0')}"
        }
    }
}

/**
 * Where a move may go: anywhere for the owner (null); for others, inside the share only, so
 * browsing starts at the shared folder (the first segment of the path, else the folder itself).
 */
fun moveRoot(access: AccessLevel, path: List<PathSegmentDto>): PathSegmentDto? =
    if (access == AccessLevel.owner) null else path.firstOrNull()
