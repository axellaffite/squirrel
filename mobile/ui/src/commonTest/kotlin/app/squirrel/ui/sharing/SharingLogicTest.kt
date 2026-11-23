package app.squirrel.ui.sharing

import app.squirrel.api.models.AccessLevel
import app.squirrel.api.models.CommentDto
import app.squirrel.api.models.MemberDto
import app.squirrel.api.models.PathSegmentDto
import app.squirrel.api.models.ShareDto
import app.squirrel.api.models.SharePermission
import kotlinx.datetime.TimeZone
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.time.Duration.Companion.days
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

class SharingLogicTest {
    private val anna = MemberDto(displayName = "Anna", username = "anna")
    private val at = Instant.parse("2026-11-20T17:05:00Z")

    private fun share(id: String, permission: SharePermission) = ShareDto(createdAt = at, id = id, permission = permission, person = anna)

    private fun comment(id: String) = CommentDto(body = "hi", canDelete = false, createdAt = at, id = id, mine = false)

    @Test
    fun abilitiesFollowTheAccessLevel() {
        assertEquals(Abilities(owner = true, edit = true), Abilities.of(AccessLevel.owner))
        assertEquals(Abilities(owner = false, edit = true), Abilities.of(AccessLevel.write))
        assertEquals(Abilities(owner = false, edit = false), Abilities.of(AccessLevel.read))
        assertEquals(Abilities(owner = false, edit = false), Abilities.of(AccessLevel.unknown_default_open_api))
    }

    @Test
    fun namesAndPermissions() {
        assertEquals("Anna", memberName(anna))
        assertEquals("Deleted user", memberName(null))
        assertEquals("Can edit", permissionLabel(SharePermission.write))
        assertEquals("Can view", permissionLabel(SharePermission.read))
    }

    @Test
    fun aChangedShareReplacesItsOldSelf() {
        val shares = listOf(share("a", SharePermission.read))
        assertEquals(listOf(SharePermission.write), upsertShare(shares, share("a", SharePermission.write)).map { it.permission })
        assertEquals(listOf("a", "b"), upsertShare(shares, share("b", SharePermission.read)).map { it.id })
    }

    @Test
    fun pagesOfCommentsAreMergedOnce() {
        assertEquals(listOf("1", "2", "3"), mergeComments(listOf(comment("1"), comment("2")), listOf(comment("2"), comment("3"))).map { it.id })
    }

    @Test
    fun commentsAreCheckedBeforeSending() {
        assertEquals("Write a comment first.", commentProblem("   "))
        assertEquals("Comments have at most 4000 characters.", commentProblem("x".repeat(COMMENT_MAX + 1)))
        assertNull(commentProblem(" fine "))
    }

    @Test
    fun commentDatesAreRelativeThenAbsolute() {
        val utc = TimeZone.UTC
        assertEquals("Just now", commentWhen(at, at + 30.seconds, utc))
        assertEquals("5 min ago", commentWhen(at, at + 5.minutes, utc))
        assertEquals("3 h ago", commentWhen(at, at + 3.hours, utc))
        assertEquals("20 November 2026, 17:05", commentWhen(at, at + 2.days, utc))
    }

    @Test
    fun movesStayInsideTheShareForOthers() {
        val path = listOf(PathSegmentDto("s", "Shared"), PathSegmentDto("c", "Child"))
        assertNull(moveRoot(AccessLevel.owner, path))
        assertEquals("s", moveRoot(AccessLevel.write, path)?.id)
    }

    @Test
    fun subjectsSayWhatTheyAre() {
        assertEquals("n", Subject.Node("n").nodeId)
        assertNull(Subject.Node("n").albumId)
        assertEquals("a", Subject.Album("a").albumId)
        assertEquals(true, isPhotoFile("image/heic"))
        assertEquals(false, isPhotoFile("application/pdf"))
    }
}
