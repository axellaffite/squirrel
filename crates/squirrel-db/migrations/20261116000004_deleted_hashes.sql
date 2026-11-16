-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 6: content a phone backed up and the user then deleted for good. The
-- sync check reports it as present, so no phone of that user (a new sign-in,
-- a reinstall) uploads it again. Kept for good: maintenance never deletes them.
CREATE TABLE deleted_hashes (
    owner_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    sha256      TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    deleted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (owner_id, sha256)
);

-- Permanent deletion asks whether any device recorded a hash.
CREATE INDEX device_assets_sha256_idx ON device_assets (sha256, device_id);
