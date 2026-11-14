-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 3: a Postgres job queue (claimed with FOR UPDATE SKIP LOCKED).

CREATE TABLE jobs (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind          TEXT NOT NULL CHECK (kind ~ '^[a-z][a-z0-9_]{0,63}$'),
    payload       JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Higher runs first. Interactive work ~100, backfill ~0, ML (Phase 5) below 0.
    priority      SMALLINT NOT NULL DEFAULT 0,
    -- queued → running → (deleted on success) | queued again (retry) | failed.
    status        TEXT NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'failed')),
    attempts      INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts  INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    -- Not before this time (retries back off by pushing it forward).
    run_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Visibility timeout: a running job whose lease expired goes back to the queue.
    locked_until  TIMESTAMPTZ,
    -- Fencing token of the current claim: a worker that lost its lease cannot
    -- complete, retry or extend a job another worker has claimed since.
    claim_id      UUID,
    -- Which worker holds it (for admins), e.g. 'minipc:4242'.
    claimed_by    TEXT CHECK (claimed_by IS NULL OR char_length(claimed_by) <= 200),
    last_error    TEXT CHECK (last_error IS NULL OR char_length(last_error) <= 2000),
    -- At most one queued or running job per key (e.g. 'thumbnails:<sha256>').
    dedupe_key    TEXT CHECK (dedupe_key IS NULL OR char_length(dedupe_key) <= 200),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT jobs_running_is_claimed CHECK (
        (status = 'running') = (locked_until IS NOT NULL AND claim_id IS NOT NULL))
);
-- Claim order. Partial: finished jobs are deleted, failed ones are never claimed.
CREATE INDEX jobs_claim_idx ON jobs (priority DESC, run_at, id) WHERE status = 'queued';
CREATE INDEX jobs_lease_idx ON jobs (locked_until) WHERE status = 'running';
CREATE UNIQUE INDEX jobs_dedupe_key ON jobs (dedupe_key)
    WHERE dedupe_key IS NOT NULL AND status IN ('queued', 'running');
CREATE INDEX jobs_failed_idx ON jobs (updated_at DESC) WHERE status = 'failed';

-- Wakes idle workers. Statement-level: one notification per INSERT, however many rows.
CREATE FUNCTION jobs_notify() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('squirrel_jobs', '');
    RETURN NULL;
END;
$$;
CREATE TRIGGER jobs_notify AFTER INSERT ON jobs
    FOR EACH STATEMENT EXECUTE FUNCTION jobs_notify();
