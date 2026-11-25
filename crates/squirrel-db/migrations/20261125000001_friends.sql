-- FROZEN once merged: never edit; add a new migration instead.
-- Friends: each account's one-way list of other accounts, for quick sharing.
-- Plan: docs/plans/2026-11-24-friends.md.

CREATE TABLE friends (
    user_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    friend_id   UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, friend_id),
    CONSTRAINT friends_not_self CHECK (user_id <> friend_id)
);
-- Account deletion cascades through `friend_id` too.
CREATE INDEX friends_friend_idx ON friends (friend_id);
