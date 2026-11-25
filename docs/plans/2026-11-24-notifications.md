# Notifications

Date: 2026-11-24. Status: built. Tells people when something is shared with them or commented on.

## Decisions (validated with the user)

- **In-app inbox** only: a bell with an unread count on web and phone, a list, mark read. The phone also
  raises an OS notification for new unread items when its background sync runs. No Web Push, no APNs/FCM.
- **New share** → its recipient. A permission change on an existing share notifies no one.
- **New comment** → everyone who can see the subject, except the author: the owner, recipients of a share
  on the subject or any ancestor folder, and for a photo, the owners and recipients of albums holding it.
  Disabled accounts are skipped. Edits notify no one.

## Model

- `notifications(id, user_id, kind share|comment, actor_id NULL on account deletion, node_id | album_id,
  share_id | comment_id, created_at, read_at)`. FKs cascade: deleting the share, comment or subject deletes
  the notification.
- Created in the same request as the share or comment (best effort: a failure is logged, never fails the
  request).

## Risks

- **Stale access**: a notification outlives an ancestor share being revoked or the subject being trashed.
  The list re-checks access per item through `access` and deletes the ones no longer visible, so a revoked
  person never sees the subject name or comment text. The unread count may stay one too high until then.
- Fan-out on a widely shared folder: one INSERT … SELECT per comment, bounded by the number of accounts.

## Tasks

1. Migration + `squirrel-db::notifications` (create for share / comment, list, unread count, mark read), DB tests.
2. API: `GET /api/notifications`, `GET /api/notifications/unread-count`, `POST /api/notifications/read`;
   hooks in share and comment creation; OpenAPI; API tests.
3. Web: bell + panel in the shell, links to the subject; E2E.
4. Phone: bell + sheet, OS notification from the background sync.

## As built

- Migration `20261124000001_notifications.sql`; `squirrel_db::notifications`; routes in `squirrel-api/src/routes/notifications.rs` (session or phone token). One notification per share (unique `share_id`), per comment and person.
- Web: bell in the rail (desktop) or top row (phone), popover/sheet, 60 s poll while visible; links reuse `?open=` and add `comments=1` to open the thread. E2E `e2e/notifications.spec.ts`.
- Phone: bell + sheet + LS-DO screen `notifications`; OS notifications after each background backup run (Android WorkManager, iOS BGTask), deduplicated by a mark in settings, up to 3 then one summary; permission asked on the first bell tap.
- Not done: scrolling to the exact comment, deep links from OS notifications, announcing when automatic backup is off. OS notifications untested on a real device.
