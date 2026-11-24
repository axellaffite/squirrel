package app.squirrel.shared.notifications

import app.squirrel.api.models.MemberDto
import app.squirrel.api.models.NotificationDto
import app.squirrel.api.models.NotificationKind
import app.squirrel.api.models.NotificationSubjectDto
import app.squirrel.api.models.SharePermission
import app.squirrel.api.models.SubjectKind
import kotlin.time.Instant

fun share(id: String, at: String = "2026-11-24T10:00:00Z", read: Boolean = false, permission: SharePermission? = SharePermission.write) = NotificationDto(
    createdAt = Instant.parse(at),
    id = id,
    kind = NotificationKind.share,
    read = read,
    subject = NotificationSubjectDto(id = "f-$id", kind = SubjectKind.folder, name = "Trip"),
    actor = MemberDto(displayName = "Anna", username = "anna"),
    permission = permission,
)

fun comment(id: String, at: String = "2026-11-24T10:00:00Z", read: Boolean = false, excerpt: String? = "Nice") = NotificationDto(
    createdAt = Instant.parse(at),
    id = id,
    kind = NotificationKind.comment,
    read = read,
    subject = NotificationSubjectDto(id = "n-$id", kind = SubjectKind.file, name = "beach.jpg", mime = "image/jpeg"),
    actor = MemberDto(displayName = "Ben", username = "ben"),
    excerpt = excerpt,
    commentId = "c-$id",
)
