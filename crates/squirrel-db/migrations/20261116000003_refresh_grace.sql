-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 6: a refresh replayed shortly after its rotation (two requests racing,
-- or a lost response) gets the same new refresh token instead of revoking the
-- device. The new token is derived from the presented one and this salt, so
-- it is never stored in clear.
ALTER TABLE devices
    ADD COLUMN rotated_at TIMESTAMPTZ,
    ADD COLUMN rotation_salt BYTEA,
    ADD CONSTRAINT devices_rotation_shape CHECK ((rotated_at IS NULL) = (rotation_salt IS NULL));
