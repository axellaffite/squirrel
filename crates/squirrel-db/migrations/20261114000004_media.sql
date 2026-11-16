-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 3: what the worker learns from a blob's bytes (`media`), and the
-- per-user timeline entries (`assets`), one per image file node.

-- Content-derived facts. Shared by every node, of any user, that references
-- the blob; only ever read through a node the caller owns.
CREATE TABLE media (
    sha256           TEXT PRIMARY KEY REFERENCES blobs (sha256) ON DELETE CASCADE,
    kind             TEXT NOT NULL CHECK (kind IN ('image', 'pdf', 'text')),
    -- Each step: pending → ready | failed | unsupported (nothing to do for this format).
    metadata_state   TEXT NOT NULL DEFAULT 'pending'
                     CHECK (metadata_state IN ('pending', 'ready', 'failed', 'unsupported')),
    thumbs_state     TEXT NOT NULL DEFAULT 'pending'
                     CHECK (thumbs_state IN ('pending', 'ready', 'failed', 'unsupported')),
    text_state       TEXT NOT NULL DEFAULT 'unsupported'
                     CHECK (text_state IN ('pending', 'ready', 'failed', 'unsupported')),
    -- Bumped when thumbnails are regenerated with new settings; part of their URL.
    thumbs_version   SMALLINT NOT NULL DEFAULT 0 CHECK (thumbs_version >= 0),
    -- As displayed (EXIF orientation applied).
    width            INTEGER CHECK (width > 0),
    height           INTEGER CHECK (height > 0),
    blurhash         TEXT CHECK (char_length(blurhash) <= 64),
    -- Wall-clock capture time and, when the file records it, its UTC offset.
    exif_taken_at    TIMESTAMP,
    exif_offset_min  SMALLINT CHECK (exif_offset_min BETWEEN -1080 AND 1080),
    latitude         DOUBLE PRECISION CHECK (latitude BETWEEN -90 AND 90),
    longitude        DOUBLE PRECISION CHECK (longitude BETWEEN -180 AND 180),
    altitude         DOUBLE PRECISION,
    camera_make      TEXT CHECK (char_length(camera_make) <= 128),
    camera_model     TEXT CHECK (char_length(camera_model) <= 128),
    lens_model       TEXT CHECK (char_length(lens_model) <= 128),
    exposure_time    TEXT CHECK (char_length(exposure_time) <= 32),
    f_number         REAL,
    iso              INTEGER,
    focal_length_mm  REAL,
    -- The EXIF says "Screenshot" (names and shapes are judged per node, in `assets.kind`).
    is_screenshot    BOOLEAN NOT NULL DEFAULT FALSE,
    page_count       INTEGER CHECK (page_count >= 0),
    -- Reverse geocoding (GeoNames); `geocoded_with` names the dataset used.
    place_name       TEXT,
    place_admin1     TEXT,
    place_country    TEXT,
    place_country_code TEXT CHECK (place_country_code ~ '^[A-Z]{2}$'),
    geocoded_with    TEXT,
    -- Last failure of any step, for admins.
    last_error       TEXT CHECK (char_length(last_error) <= 2000),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((latitude IS NULL) = (longitude IS NULL))
);

-- One timeline entry per image file node. `taken_at` is the wall-clock time
-- shown to the user and the sort key (days and months group on it).
CREATE TABLE assets (
    node_id       UUID PRIMARY KEY,
    owner_id      UUID NOT NULL,
    sha256        TEXT NOT NULL REFERENCES media (sha256),
    kind          TEXT NOT NULL CHECK (kind IN ('photo', 'screenshot')),
    taken_at      TIMESTAMP NOT NULL,
    -- UTC offset of `taken_at` in minutes, when known.
    taken_offset_min SMALLINT CHECK (taken_offset_min BETWEEN -1080 AND 1080),
    taken_source  TEXT NOT NULL CHECK (taken_source IN ('exif', 'filename', 'upload')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT assets_node_owner_key UNIQUE (node_id, owner_id),
    CONSTRAINT assets_node_fkey FOREIGN KEY (node_id, owner_id)
        REFERENCES nodes (id, owner_id) ON DELETE CASCADE
);
-- Timeline keyset pagination: ORDER BY taken_at DESC, node_id DESC.
CREATE INDEX assets_timeline_idx ON assets (owner_id, taken_at DESC, node_id DESC);
CREATE INDEX assets_sha256_idx ON assets (sha256);
