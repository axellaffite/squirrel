-- FROZEN once merged: never edit; add a new migration instead.
-- In-app notifications: something was shared with you, or commented on.
-- Plan: docs/plans/2026-11-24-notifications.md.

CREATE TABLE notifications (
    id          UUID PRIMARY KEY,
    -- Who is told.
    user_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('share', 'comment')),
    -- Who shared or commented; NULL once their account is deleted.
    actor_id    UUID REFERENCES users (id) ON DELETE SET NULL,
    node_id     UUID REFERENCES nodes (id) ON DELETE CASCADE,
    album_id    UUID REFERENCES albums (id) ON DELETE CASCADE,
    share_id    UUID REFERENCES shares (id) ON DELETE CASCADE,
    comment_id  UUID REFERENCES comments (id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    read_at     TIMESTAMPTZ,
    CONSTRAINT notifications_one_subject CHECK ((node_id IS NULL) <> (album_id IS NULL)),
    CONSTRAINT notifications_cause CHECK (
        (kind = 'share' AND share_id IS NOT NULL AND comment_id IS NULL)
        OR (kind = 'comment' AND comment_id IS NOT NULL AND share_id IS NULL))
);
-- A share is announced once, whatever its later permission changes.
CREATE UNIQUE INDEX notifications_share_key ON notifications (share_id) WHERE share_id IS NOT NULL;
CREATE UNIQUE INDEX notifications_comment_user_key ON notifications (comment_id, user_id)
    WHERE comment_id IS NOT NULL;
-- The inbox, newest first.
CREATE INDEX notifications_user_idx ON notifications (user_id, created_at DESC, id DESC);
CREATE INDEX notifications_unread_idx ON notifications (user_id) WHERE read_at IS NULL;
CREATE INDEX notifications_actor_idx ON notifications (actor_id);
CREATE INDEX notifications_node_idx ON notifications (node_id) WHERE node_id IS NOT NULL;
CREATE INDEX notifications_album_idx ON notifications (album_id) WHERE album_id IS NOT NULL;
