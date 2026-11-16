-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 6: which phone asset became which content, and the phone's capture time.

-- Backup-only: rows are never removed when the phone deletes the asset.
CREATE TABLE device_assets (
    device_id        UUID NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    device_asset_id  TEXT NOT NULL CHECK (octet_length(device_asset_id) BETWEEN 1 AND 256),
    sha256           TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    node_id          UUID REFERENCES nodes (id) ON DELETE SET NULL,
    synced_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (device_id, device_asset_id)
);
-- `ON DELETE SET NULL` from nodes looks rows up by node.
CREATE INDEX device_assets_node_idx ON device_assets (node_id) WHERE node_id IS NOT NULL;

-- The capture time a phone reported (wall clock + UTC offset), step 3 of the taken_at chain.
ALTER TABLE nodes
    ADD COLUMN client_taken_at TIMESTAMP,
    ADD COLUMN client_taken_offset_min SMALLINT
        CHECK (client_taken_offset_min BETWEEN -1080 AND 1080),
    ADD CONSTRAINT nodes_client_taken_shape
        CHECK (client_taken_offset_min IS NULL OR client_taken_at IS NOT NULL);

-- Carried by the upload session until completion creates the node.
ALTER TABLE upload_sessions
    ADD COLUMN client_taken_at TIMESTAMP,
    ADD COLUMN client_taken_offset_min SMALLINT
        CHECK (client_taken_offset_min BETWEEN -1080 AND 1080),
    ADD COLUMN device_id UUID REFERENCES devices (id) ON DELETE SET NULL,
    ADD COLUMN device_asset_id TEXT
        CHECK (device_asset_id IS NULL OR octet_length(device_asset_id) BETWEEN 1 AND 256),
    -- A deleted device leaves the asset id behind; completion then records nothing.
    ADD CONSTRAINT upload_sessions_device_asset_shape
        CHECK (device_id IS NULL OR device_asset_id IS NOT NULL),
    ADD CONSTRAINT upload_sessions_client_taken_shape
        CHECK (client_taken_offset_min IS NULL OR client_taken_at IS NOT NULL);
CREATE INDEX upload_sessions_device_idx ON upload_sessions (device_id) WHERE device_id IS NOT NULL;

-- The phone's time is a fourth source (name checked with \d assets: assets_taken_source_check).
ALTER TABLE assets DROP CONSTRAINT assets_taken_source_check;
ALTER TABLE assets ADD CONSTRAINT assets_taken_source_check
    CHECK (taken_source IN ('exif', 'filename', 'device', 'upload'));
