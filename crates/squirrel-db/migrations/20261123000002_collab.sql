-- FROZEN once merged: never edit; add a new migration instead.
-- Live co-editing of text files. Plan: docs/plans/2026-11-23-text-editor.md.

-- The Yjs state of a file being co-edited, valid while the file's blob is
-- `blob_sha256`. When the file changed outside the editor the state is rebuilt
-- from the file and `epoch` changes, so clients drop their stale copies.
CREATE TABLE collab_docs (
    node_id      UUID PRIMARY KEY REFERENCES nodes (id) ON DELETE CASCADE,
    epoch        UUID NOT NULL,
    blob_sha256  TEXT NOT NULL CHECK (blob_sha256 ~ '^[0-9a-f]{64}$'),
    state        BYTEA NOT NULL,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
