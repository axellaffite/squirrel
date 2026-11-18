-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 6 (iOS): resuming an upload rotates its token, but chunks already handed
-- to the phone's background session carry the token they were queued with. The
-- token before the current one stays valid (same scope, same deaths), so a
-- relaunch does not turn those transfers into a stream of 401s. One rotation
-- later it is gone.
ALTER TABLE upload_sessions
    ADD COLUMN previous_upload_token_hash BYTEA UNIQUE,
    ADD COLUMN previous_upload_token_device_id UUID REFERENCES devices (id) ON DELETE SET NULL;
CREATE INDEX upload_sessions_previous_token_device_idx
    ON upload_sessions (previous_upload_token_device_id)
    WHERE previous_upload_token_device_id IS NOT NULL;
