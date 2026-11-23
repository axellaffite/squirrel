-- FROZEN once merged: never edit; add a new migration instead.
-- Sharing folders, files and albums with other accounts, and comments on them.
-- Plan: docs/plans/2026-11-23-sharing.md.

-- A share gives `recipient_id` read or write access to one of `owner_id`'s
-- folders, files (the subtree for a folder) or albums.
CREATE TABLE shares (
    id            UUID PRIMARY KEY,
    owner_id      UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    recipient_id  UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    node_id       UUID,
    album_id      UUID,
    permission    TEXT NOT NULL CHECK (permission IN ('read', 'write')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT shares_one_subject CHECK ((node_id IS NULL) <> (album_id IS NULL)),
    CONSTRAINT shares_not_self CHECK (owner_id <> recipient_id),
    CONSTRAINT shares_node_fkey FOREIGN KEY (node_id, owner_id)
        REFERENCES nodes (id, owner_id) ON DELETE CASCADE,
    CONSTRAINT shares_album_fkey FOREIGN KEY (album_id, owner_id)
        REFERENCES albums (id, owner_id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX shares_node_recipient_key ON shares (node_id, recipient_id)
    WHERE node_id IS NOT NULL;
CREATE UNIQUE INDEX shares_album_recipient_key ON shares (album_id, recipient_id)
    WHERE album_id IS NOT NULL;
-- "Shared with me", newest first.
CREATE INDEX shares_recipient_idx ON shares (recipient_id, created_at DESC, id DESC);
-- "Shared by me" and account deletion.
CREATE INDEX shares_owner_idx ON shares (owner_id, created_at DESC);

-- One flat thread per folder, file or album. `owner_id` is the subject's owner.
CREATE TABLE comments (
    id          UUID PRIMARY KEY,
    owner_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    node_id     UUID,
    album_id    UUID,
    -- NULL once the author's account is deleted: the comment stays.
    author_id   UUID REFERENCES users (id) ON DELETE SET NULL,
    body        TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 4000),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at   TIMESTAMPTZ,
    CONSTRAINT comments_one_subject CHECK ((node_id IS NULL) <> (album_id IS NULL)),
    CONSTRAINT comments_node_fkey FOREIGN KEY (node_id, owner_id)
        REFERENCES nodes (id, owner_id) ON DELETE CASCADE,
    CONSTRAINT comments_album_fkey FOREIGN KEY (album_id, owner_id)
        REFERENCES albums (id, owner_id) ON DELETE CASCADE
);
-- Threads in posting order.
CREATE INDEX comments_node_idx ON comments (node_id, created_at, id) WHERE node_id IS NOT NULL;
CREATE INDEX comments_album_idx ON comments (album_id, created_at, id) WHERE album_id IS NOT NULL;
CREATE INDEX comments_author_idx ON comments (author_id);

-- Albums may hold photos of the people they are shared with for writing.
-- `album_items.owner_id` now means the photo's owner (who added it); the
-- album's owner is `albums.owner_id`. Existing rows are unchanged (same owner).
ALTER TABLE album_items DROP CONSTRAINT album_items_album_fkey;
ALTER TABLE album_items ADD CONSTRAINT album_items_album_fkey
    FOREIGN KEY (album_id) REFERENCES albums (id) ON DELETE CASCADE;
-- Revoking a write share removes that person's photos from the album.
CREATE INDEX album_items_album_owner_idx ON album_items (album_id, owner_id);

-- Uploads into a folder shared for writing: the session writes into
-- `owner_id`'s tree (and quota) but belongs to `created_by`, the uploader.
ALTER TABLE upload_sessions ADD COLUMN created_by UUID REFERENCES users (id) ON DELETE CASCADE;
UPDATE upload_sessions SET created_by = owner_id;
ALTER TABLE upload_sessions ALTER COLUMN created_by SET NOT NULL;
CREATE INDEX upload_sessions_created_by_idx ON upload_sessions (created_by, parent_id, sha256);
