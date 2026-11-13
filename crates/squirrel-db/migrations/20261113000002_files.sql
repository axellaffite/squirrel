-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 2: content-addressed blobs, the per-user file tree, and upload sessions.

CREATE TABLE blobs (
    sha256      TEXT PRIMARY KEY CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    size        BIGINT NOT NULL CHECK (size >= 0),
    mime        TEXT NOT NULL CHECK (char_length(mime) BETWEEN 1 AND 255),
    -- Maintained by the nodes_blob_ref* triggers; never update by hand.
    ref_count   BIGINT NOT NULL DEFAULT 0 CHECK (ref_count >= 0),
    -- When ref_count last reached 0 (NULL while referenced). GC waits for a grace period.
    zero_since  TIMESTAMPTZ DEFAULT now(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((ref_count = 0) = (zero_since IS NOT NULL))
);
CREATE INDEX blobs_unreferenced_idx ON blobs (zero_since) WHERE ref_count = 0;

CREATE TABLE nodes (
    id           UUID PRIMARY KEY,
    owner_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    parent_id    UUID,
    name         TEXT NOT NULL CHECK (octet_length(name) BETWEEN 1 AND 255),
    kind         TEXT NOT NULL CHECK (kind IN ('folder', 'file')),
    blob_sha256  TEXT REFERENCES blobs (sha256),
    size         BIGINT NOT NULL DEFAULT 0 CHECK (size >= 0),
    mime         TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at   TIMESTAMPTZ,
    -- Id of the node whose deletion put this one in the trash (itself for a trash entry).
    trash_root   UUID,
    CONSTRAINT nodes_file_has_blob CHECK ((kind = 'file') = (blob_sha256 IS NOT NULL)),
    CONSTRAINT nodes_folder_shape CHECK (kind = 'file' OR (size = 0 AND mime IS NULL)),
    CONSTRAINT nodes_roots_are_folders CHECK (parent_id IS NOT NULL OR kind = 'folder'),
    CONSTRAINT nodes_trash_consistent CHECK ((deleted_at IS NULL) = (trash_root IS NULL)),
    CONSTRAINT nodes_not_own_parent CHECK (parent_id <> id),
    CONSTRAINT nodes_id_owner_key UNIQUE (id, owner_id),
    -- A parent belongs to the same owner. MATCH SIMPLE: roots (NULL parent) skip the check.
    CONSTRAINT nodes_parent_fkey FOREIGN KEY (parent_id, owner_id)
        REFERENCES nodes (id, owner_id) ON DELETE CASCADE
);
-- One live name per folder; NULLS NOT DISTINCT makes top-level (NULL parent) names unique too.
CREATE UNIQUE INDEX nodes_live_name_key ON nodes (owner_id, parent_id, name)
    NULLS NOT DISTINCT WHERE deleted_at IS NULL;
-- Folder listings: folders first, then names in byte order. Keyset pagination
-- compares ((kind = 'file'), name COLLATE "C") as a row, an Index Cond here.
CREATE INDEX nodes_children_idx ON nodes (owner_id, parent_id, (kind = 'file'), name COLLATE "C")
    WHERE deleted_at IS NULL;
CREATE INDEX nodes_parent_idx ON nodes (parent_id);
CREATE INDEX nodes_owner_blob_idx ON nodes (owner_id, blob_sha256) WHERE blob_sha256 IS NOT NULL;
CREATE INDEX nodes_blob_idx ON nodes (blob_sha256) WHERE blob_sha256 IS NOT NULL;
-- Quota: index-only SUM(size) over an owner's live files.
CREATE INDEX nodes_live_size_idx ON nodes (owner_id) INCLUDE (size)
    WHERE kind = 'file' AND deleted_at IS NULL;
-- Trash listing (matches ORDER BY deleted_at DESC, id DESC).
CREATE INDEX nodes_trash_idx ON nodes (owner_id, deleted_at DESC, id DESC) WHERE trash_root = id;
-- Whole trash batches (restore, size, permanent deletion).
CREATE INDEX nodes_trash_batch_idx ON nodes (owner_id, trash_root) WHERE trash_root IS NOT NULL;
-- Global purge of old entries, oldest first; covering so it stays index-only.
CREATE INDEX nodes_trash_purge_idx ON nodes (deleted_at, id) INCLUDE (owner_id) WHERE trash_root = id;

-- Blob reference counting. Inserts and deletes use statement-level triggers:
-- one UPDATE per statement, aggregated per blob, locking blobs in sha256 order
-- so concurrent bulk deletes sharing blobs do not deadlock on each other.
-- `zero_since` follows the CHECK above: set when ref_count reaches 0, else NULL.
CREATE FUNCTION nodes_blob_refs_added() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM 1 FROM blobs
     WHERE sha256 IN (SELECT blob_sha256 FROM new_rows WHERE blob_sha256 IS NOT NULL)
     ORDER BY sha256 FOR NO KEY UPDATE;
    UPDATE blobs b SET ref_count = b.ref_count + d.n, zero_since = NULL
      FROM (SELECT blob_sha256, count(*) AS n FROM new_rows
             WHERE blob_sha256 IS NOT NULL GROUP BY blob_sha256) d
     WHERE b.sha256 = d.blob_sha256;
    RETURN NULL;
END;
$$;
CREATE FUNCTION nodes_blob_refs_removed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM 1 FROM blobs
     WHERE sha256 IN (SELECT blob_sha256 FROM old_rows WHERE blob_sha256 IS NOT NULL)
     ORDER BY sha256 FOR NO KEY UPDATE;
    UPDATE blobs b
       SET ref_count = b.ref_count - d.n,
           zero_since = CASE WHEN b.ref_count = d.n THEN now() ELSE NULL END
      FROM (SELECT blob_sha256, count(*) AS n FROM old_rows
             WHERE blob_sha256 IS NOT NULL GROUP BY blob_sha256) d
     WHERE b.sha256 = d.blob_sha256;
    RETURN NULL;
END;
$$;
-- Row level for the rare in-place change of a node's blob.
CREATE FUNCTION nodes_blob_ref_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.blob_sha256 IS NOT NULL THEN
        UPDATE blobs
           SET ref_count = ref_count - 1,
               zero_since = CASE WHEN ref_count = 1 THEN now() ELSE NULL END
         WHERE sha256 = OLD.blob_sha256;
    END IF;
    IF NEW.blob_sha256 IS NOT NULL THEN
        UPDATE blobs SET ref_count = ref_count + 1, zero_since = NULL
         WHERE sha256 = NEW.blob_sha256;
    END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER nodes_blob_refs_added
    AFTER INSERT ON nodes REFERENCING NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION nodes_blob_refs_added();
CREATE TRIGGER nodes_blob_refs_removed
    AFTER DELETE ON nodes REFERENCING OLD TABLE AS old_rows
    FOR EACH STATEMENT EXECUTE FUNCTION nodes_blob_refs_removed();
CREATE TRIGGER nodes_blob_ref_changed
    AFTER UPDATE OF blob_sha256 ON nodes
    FOR EACH ROW WHEN (OLD.blob_sha256 IS DISTINCT FROM NEW.blob_sha256)
    EXECUTE FUNCTION nodes_blob_ref_changed();

CREATE TABLE upload_sessions (
    id           UUID PRIMARY KEY,
    owner_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    parent_id    UUID NOT NULL,
    name         TEXT NOT NULL CHECK (octet_length(name) BETWEEN 1 AND 255),
    sha256       TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    size         BIGINT NOT NULL CHECK (size >= 0),
    chunk_size   BIGINT NOT NULL CHECK (chunk_size > 0),
    chunk_count  INTEGER NOT NULL CHECK (chunk_count >= 0),
    -- 'completing' while one request verifies and stores the file.
    status       TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'completing')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Pushed back on every chunk: sessions expire after 24 h without activity.
    expires_at   TIMESTAMPTZ NOT NULL,
    CONSTRAINT upload_sessions_parent_fkey FOREIGN KEY (parent_id, owner_id)
        REFERENCES nodes (id, owner_id) ON DELETE CASCADE,
    -- chunk_count = ceil(size / chunk_size), written without overflow (0 for an empty file).
    CONSTRAINT upload_sessions_chunk_geometry CHECK (
        chunk_count = size / chunk_size + CASE WHEN size % chunk_size > 0 THEN 1 ELSE 0 END)
);
CREATE INDEX upload_sessions_owner_idx ON upload_sessions (owner_id, parent_id, sha256);
CREATE INDEX upload_sessions_expires_idx ON upload_sessions (expires_at);
CREATE INDEX upload_sessions_parent_idx ON upload_sessions (parent_id);

CREATE TABLE upload_chunks (
    upload_id    UUID NOT NULL REFERENCES upload_sessions (id) ON DELETE CASCADE,
    chunk_index  INTEGER NOT NULL CHECK (chunk_index >= 0),
    PRIMARY KEY (upload_id, chunk_index)
);

-- Every existing user gets the default roots (new users get them from the application).
INSERT INTO nodes (id, owner_id, parent_id, name, kind)
SELECT gen_random_uuid(), u.id, NULL, r.name, 'folder'
  FROM users u CROSS JOIN (VALUES ('Photos'), ('Documents'), ('Files')) AS r (name);
