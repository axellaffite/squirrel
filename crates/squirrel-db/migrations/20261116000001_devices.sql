-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 6: phones signed in with bearer tokens. Tokens are stored as SHA-256 only.

CREATE TABLE devices (
    id                     UUID PRIMARY KEY,
    owner_id               UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name                   TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 64),
    platform               TEXT NOT NULL CHECK (platform IN ('android', 'ios')),
    model                  TEXT CHECK (model IS NULL OR char_length(model) BETWEEN 1 AND 64),
    app_version            TEXT NOT NULL CHECK (char_length(app_version) BETWEEN 1 AND 32),
    refresh_hash           BYTEA NOT NULL UNIQUE,
    -- Kept until the current refresh token is first used (a lost response can be retried).
    previous_refresh_hash  BYTEA UNIQUE,
    -- Sliding: pushed back on every refresh.
    refresh_expires_at     TIMESTAMPTZ NOT NULL,
    -- Photos/<name>, created on first use by POST /api/sync/folder.
    backup_folder_id       UUID,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at             TIMESTAMPTZ,
    CONSTRAINT devices_id_owner_key UNIQUE (id, owner_id),
    CONSTRAINT devices_backup_folder_fkey FOREIGN KEY (backup_folder_id, owner_id)
        REFERENCES nodes (id, owner_id) ON DELETE SET NULL (backup_folder_id)
);
CREATE INDEX devices_owner_idx ON devices (owner_id, revoked_at NULLS FIRST, last_seen_at DESC);
CREATE INDEX devices_backup_folder_idx ON devices (backup_folder_id) WHERE backup_folder_id IS NOT NULL;
-- Maintenance: devices revoked or idle long ago are deleted.
CREATE INDEX devices_revoked_idx ON devices (revoked_at) WHERE revoked_at IS NOT NULL;
CREATE INDEX devices_refresh_expires_idx ON devices (refresh_expires_at);

-- Several may be alive at once (a background worker and the UI refreshing concurrently).
CREATE TABLE device_access_tokens (
    token_hash  BYTEA PRIMARY KEY,
    device_id   UUID NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX device_access_tokens_device_idx ON device_access_tokens (device_id);
CREATE INDEX device_access_tokens_expires_idx ON device_access_tokens (expires_at);

-- Refresh tokens that were rotated out twice. Presenting one is reuse: the device is revoked.
CREATE TABLE device_retired_refresh (
    token_hash  BYTEA PRIMARY KEY,
    device_id   UUID NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    -- Rows older than the refresh lifetime are pruned by maintenance.
    retired_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX device_retired_refresh_device_idx ON device_retired_refresh (device_id);
CREATE INDEX device_retired_refresh_retired_idx ON device_retired_refresh (retired_at);
