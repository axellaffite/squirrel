-- FROZEN once merged: never edit; add a new migration instead.
-- Identifies this installation. The server writes the id into the data
-- directory (.squirrel-instance) and refuses to start when the two disagree,
-- so a data directory is never served (or swept) against another database.
CREATE TABLE instance_identity (
    id               BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    installation_id  UUID NOT NULL DEFAULT gen_random_uuid(),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO instance_identity DEFAULT VALUES;
