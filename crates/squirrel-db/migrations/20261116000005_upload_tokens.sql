-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 6 (iOS): a capability for one upload's chunks and status, given to the
-- device that opened or resumed it, so background transfers outlive the
-- 15-minute access token. Stored hashed; dies with the session, and works only
-- while that device is signed in.
ALTER TABLE upload_sessions
    ADD COLUMN upload_token_hash BYTEA UNIQUE,
    ADD COLUMN upload_token_device_id UUID REFERENCES devices (id) ON DELETE SET NULL;
CREATE INDEX upload_sessions_token_device_idx ON upload_sessions (upload_token_device_id)
    WHERE upload_token_device_id IS NOT NULL;
