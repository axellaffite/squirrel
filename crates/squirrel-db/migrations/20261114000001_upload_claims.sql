-- When the current completion claimed the session. A `completing` session whose
-- claim is older than the lease (10 minutes) was abandoned by a crashed or
-- cancelled completion and may be claimed again.
ALTER TABLE upload_sessions ADD COLUMN claimed_at TIMESTAMPTZ;
