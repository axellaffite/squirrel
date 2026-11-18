-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 5: faces and people. Per owner: biometric data is never shared between
-- users, even for a deduplicated blob, and turning faces off deletes it all.
CREATE TABLE people (
    id          UUID PRIMARY KEY,
    owner_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        TEXT CHECK (name IS NULL OR char_length(name) BETWEEN 1 AND 100),
    hidden      BOOLEAN NOT NULL DEFAULT FALSE,
    -- Chosen cover face; NULL = the best-scoring face.
    cover_face_id UUID,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT people_id_owner_key UNIQUE (id, owner_id)
);
CREATE INDEX people_owner_idx ON people (owner_id, hidden, name);

CREATE TABLE faces (
    id          UUID PRIMARY KEY,
    owner_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    sha256      TEXT NOT NULL REFERENCES media (sha256) ON DELETE CASCADE,
    model       TEXT NOT NULL,
    -- Relative to the displayed image (0..1), so any thumbnail size can crop it.
    box_x REAL NOT NULL, box_y REAL NOT NULL, box_w REAL NOT NULL, box_h REAL NOT NULL,
    landmarks   REAL[] NOT NULL CHECK (cardinality(landmarks) = 10),
    score       REAL NOT NULL,
    embedding   halfvec(128) NOT NULL,          -- SFace
    person_id   UUID,
    -- 'auto' (clustering may move it), 'user' (pinned to person_id, or pinned
    -- to "nobody" when person_id IS NULL: never clustered again).
    assigned_by TEXT NOT NULL DEFAULT 'auto' CHECK (assigned_by IN ('auto', 'user')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT faces_id_owner_key UNIQUE (id, owner_id),
    CONSTRAINT faces_person_fkey FOREIGN KEY (person_id, owner_id)
        REFERENCES people (id, owner_id) ON DELETE SET NULL (person_id)
);
CREATE INDEX faces_owner_person_idx ON faces (owner_id, person_id);
CREATE INDEX faces_owner_sha_idx ON faces (owner_id, sha256);
CREATE INDEX faces_unclustered_idx ON faces (owner_id, created_at) WHERE person_id IS NULL AND assigned_by = 'auto';
-- No ANN index: neighbour searches are exact, per owner (a few thousand faces).

ALTER TABLE people ADD CONSTRAINT people_cover_fkey FOREIGN KEY (cover_face_id, owner_id)
    REFERENCES faces (id, owner_id) ON DELETE SET NULL (cover_face_id);

-- Which (owner, blob) pairs were scanned with which model (also "no face found").
CREATE TABLE face_scans (
    owner_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    sha256      TEXT NOT NULL REFERENCES media (sha256) ON DELETE CASCADE,
    model       TEXT NOT NULL,
    face_count  SMALLINT NOT NULL CHECK (face_count >= 0),
    scanned_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (owner_id, sha256)
);

-- When an owner's last asset of a blob goes (permanent deletion), its faces and
-- scan go too: no biometric data outlives the photo. Trashed photos keep theirs.
CREATE FUNCTION assets_drop_faces() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM assets WHERE owner_id = OLD.owner_id AND sha256 = OLD.sha256) THEN
        DELETE FROM faces WHERE owner_id = OLD.owner_id AND sha256 = OLD.sha256;
        DELETE FROM face_scans WHERE owner_id = OLD.owner_id AND sha256 = OLD.sha256;
    END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER assets_drop_faces AFTER DELETE ON assets
    FOR EACH ROW EXECUTE FUNCTION assets_drop_faces();
