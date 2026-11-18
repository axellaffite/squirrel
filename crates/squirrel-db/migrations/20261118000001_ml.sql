-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 5: CLIP embeddings and tags (content-derived), per-user ML settings.
-- pgvector is not a trusted extension: the migration role must be a superuser,
-- or a superuser must have run CREATE EXTENSION vector beforehand.
CREATE EXTENSION IF NOT EXISTS vector;

ALTER TABLE media
    ADD COLUMN clip_state  TEXT NOT NULL DEFAULT 'pending'
        CHECK (clip_state IN ('pending', 'ready', 'failed', 'unsupported')),
    ADD COLUMN faces_state TEXT NOT NULL DEFAULT 'pending'
        CHECK (faces_state IN ('pending', 'ready', 'failed', 'unsupported'));
-- Only images are analysed (video keyframes are Phase 4's).
UPDATE media SET clip_state = 'unsupported', faces_state = 'unsupported' WHERE kind <> 'image';

-- One CLIP image embedding per blob and model. Content-derived: shared by every
-- owner of the blob, only ever read through the caller's own assets.
CREATE TABLE clip_embeddings (
    sha256     TEXT NOT NULL REFERENCES media (sha256) ON DELETE CASCADE,
    model      TEXT NOT NULL CHECK (char_length(model) <= 64),
    -- OpenAI CLIP ViT-B/32: 512 dimensions. halfvec(512) is 1 032 bytes: stored inline.
    embedding  halfvec(512) NOT NULL,
    -- The label bank version the blob's `clip` tags were computed with
    -- (NULL: not tagged yet, e.g. no label bank when it was embedded).
    tagged_labels_version SMALLINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (sha256, model)
);
-- No ANN index on purpose: searches scan the caller's own embeddings exactly.

-- Text embeddings of the label bank, per model and labels version.
CREATE TABLE clip_label_embeddings (
    model          TEXT NOT NULL CHECK (char_length(model) <= 64),
    labels_version SMALLINT NOT NULL,
    tag            TEXT NOT NULL CHECK (tag ~ '^[a-z][a-z0-9_]{0,47}$'),
    embedding      halfvec(512) NOT NULL,
    PRIMARY KEY (model, labels_version, tag)
);

-- Zero-shot tags (Phase 5) and, later, other sources. `tag` is a stable English
-- slug from the label bank ('beach', 'dog'); display names are translated in the UI.
CREATE TABLE asset_tags (
    sha256         TEXT NOT NULL REFERENCES media (sha256) ON DELETE CASCADE,
    tag            TEXT NOT NULL CHECK (tag ~ '^[a-z][a-z0-9_]{0,47}$'),
    source         TEXT NOT NULL CHECK (source IN ('clip')),
    score          REAL NOT NULL,
    labels_version SMALLINT NOT NULL,
    PRIMARY KEY (sha256, tag, source)
);
CREATE INDEX asset_tags_tag_idx ON asset_tags (tag, sha256);

-- Per-user ML preferences (one row per user, created lazily; absent = defaults).
CREATE TABLE user_ml_settings (
    user_id        UUID PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    faces_enabled  BOOLEAN NOT NULL,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
