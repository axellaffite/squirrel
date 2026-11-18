-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 5: what each worker host last reported about machine learning at start,
-- so the UI can say "Search is not set up on this server" (no paths, no details).
CREATE TABLE ml_worker_status (
    host       TEXT PRIMARY KEY CHECK (char_length(host) BETWEEN 1 AND 200),
    state      TEXT NOT NULL CHECK (state IN ('ready', 'unavailable')),
    -- A fixed, generic phrase ("models not installed"), never an error text.
    reason     TEXT CHECK (reason IS NULL OR char_length(reason) <= 200),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
