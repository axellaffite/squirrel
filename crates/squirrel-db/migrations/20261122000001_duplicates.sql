-- FROZEN once merged: never edit; add a new migration instead.
-- Duplicates: per-user groups of near-identical photos (same perceptual hash
-- within a small distance, taken within a day). Keeping one photo hides the
-- others (`assets.hidden_at`); nothing is deleted. Additive only.

-- 64-bit DCT perceptual hash of the blob's thumbnail (bits stored as BIGINT).
ALTER TABLE media ADD COLUMN phash BIGINT;

ALTER TABLE assets
    ADD COLUMN hidden_at TIMESTAMPTZ,
    ADD COLUMN hidden_reason TEXT CHECK (hidden_reason IN ('duplicate')),
    -- The grouping job has looked at this photo (reset when it leaves the trash).
    ADD COLUMN duplicates_checked BOOLEAN NOT NULL DEFAULT FALSE,
    ADD CONSTRAINT assets_hidden_pair_check CHECK ((hidden_at IS NULL) = (hidden_reason IS NULL));
CREATE INDEX assets_duplicates_unchecked_idx ON assets (owner_id, taken_at)
    WHERE NOT duplicates_checked;

CREATE TABLE duplicate_groups (
    id              UUID PRIMARY KEY,
    owner_id        UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- The anchor's facts, frozen when the group was made: a photo joins only
    -- if it is close to these (no chaining through a day of bursts).
    anchor_phash    BIGINT NOT NULL,
    anchor_taken_at TIMESTAMP NOT NULL,
    -- The photo the user kept; NULL while open, or once it left the group.
    kept_node_id    UUID,
    -- NULL = open (waiting for the user's decision).
    resolved_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT duplicate_groups_id_owner_key UNIQUE (id, owner_id)
);
CREATE INDEX duplicate_groups_anchor_idx ON duplicate_groups (owner_id, anchor_taken_at);
CREATE INDEX duplicate_groups_list_idx ON duplicate_groups (owner_id, (resolved_at IS NULL), anchor_taken_at DESC, id DESC);

-- A photo is in at most one group, of its own owner.
CREATE TABLE duplicate_members (
    node_id   UUID PRIMARY KEY,
    owner_id  UUID NOT NULL,
    group_id  UUID NOT NULL,
    -- Hamming distance to the group's anchor hash.
    distance  SMALLINT NOT NULL CHECK (distance BETWEEN 0 AND 64),
    added_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT duplicate_members_group_fkey FOREIGN KEY (group_id, owner_id)
        REFERENCES duplicate_groups (id, owner_id) ON DELETE CASCADE,
    CONSTRAINT duplicate_members_asset_fkey FOREIGN KEY (node_id, owner_id)
        REFERENCES assets (node_id, owner_id) ON DELETE CASCADE
);
CREATE INDEX duplicate_members_group_idx ON duplicate_members (group_id);

-- A photo that leaves its group (deleted for good, trashed): a group left
-- with nobody, or with one photo that is not hidden, is deleted. A group whose
-- only photo left is hidden stays, so that photo can still be restored.
CREATE FUNCTION duplicate_members_settle() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE duplicate_groups SET kept_node_id = NULL, updated_at = now()
     WHERE id = OLD.group_id AND kept_node_id = OLD.node_id;
    DELETE FROM duplicate_groups g
     WHERE g.id = OLD.group_id
       AND NOT EXISTS (SELECT 1 FROM duplicate_members m
                         JOIN assets a ON a.node_id = m.node_id
                        WHERE m.group_id = g.id
                        GROUP BY m.group_id
                       HAVING count(*) >= 2 OR bool_or(a.hidden_at IS NOT NULL));
    RETURN NULL;
END;
$$;
CREATE TRIGGER duplicate_members_settle AFTER DELETE ON duplicate_members
    FOR EACH ROW EXECUTE FUNCTION duplicate_members_settle();

-- A trashed photo leaves its group and is no longer hidden: restoring it from
-- the trash brings it back visible, to be grouped again.
CREATE FUNCTION nodes_trash_leaves_duplicates() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM duplicate_members WHERE node_id = NEW.id;
    UPDATE assets SET hidden_at = NULL, hidden_reason = NULL, duplicates_checked = FALSE
     WHERE node_id = NEW.id
       AND (hidden_at IS NOT NULL OR duplicates_checked);
    RETURN NULL;
END;
$$;
CREATE TRIGGER nodes_trash_leaves_duplicates AFTER UPDATE OF deleted_at ON nodes
    FOR EACH ROW WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL AND NEW.kind = 'file')
    EXECUTE FUNCTION nodes_trash_leaves_duplicates();
