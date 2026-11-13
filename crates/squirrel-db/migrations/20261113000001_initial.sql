CREATE TABLE users (
    id             UUID PRIMARY KEY,
    username       TEXT NOT NULL UNIQUE,
    display_name   TEXT NOT NULL,
    password_hash  TEXT NOT NULL,
    role           TEXT NOT NULL CHECK (role IN ('admin', 'user')),
    quota_bytes    BIGINT CHECK (quota_bytes IS NULL OR quota_bytes >= 0),
    invited_by     UUID REFERENCES users (id) ON DELETE SET NULL,
    failed_logins  INTEGER NOT NULL DEFAULT 0,
    locked_until   TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    disabled_at    TIMESTAMPTZ
);

CREATE TABLE invitations (
    id          UUID PRIMARY KEY,
    token_hash  BYTEA NOT NULL UNIQUE,
    created_by  UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    email_hint  TEXT,
    max_uses    INTEGER NOT NULL DEFAULT 1 CHECK (max_uses > 0),
    used_count  INTEGER NOT NULL DEFAULT 0 CHECK (used_count >= 0),
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX invitations_created_by_idx ON invitations (created_by, created_at DESC);

CREATE TABLE sessions (
    id            UUID PRIMARY KEY,
    token_hash    BYTEA NOT NULL UNIQUE,
    user_id       UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    csrf_token    TEXT NOT NULL,
    user_agent    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

CREATE TABLE instance_settings (
    id                  BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    invites_admin_only  BOOLEAN NOT NULL DEFAULT FALSE
);
INSERT INTO instance_settings DEFAULT VALUES;
