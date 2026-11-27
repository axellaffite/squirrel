-- FROZEN once merged: never edit; add a new migration instead.
-- Public links: one of an owner's folders, files or albums, readable by
-- anyone holding the link. Plan: docs/plans/2026-11-27-public-links.md.

CREATE TABLE public_links (
    id             UUID PRIMARY KEY,
    owner_id       UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    node_id        UUID,
    album_id       UUID,
    -- SHA-256 of the token in the URL; the token itself is never stored.
    token_hash     BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    -- Argon2id PHC string; NULL without a password.
    password_hash  TEXT,
    expires_at     TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT public_links_one_subject CHECK ((node_id IS NULL) <> (album_id IS NULL)),
    CONSTRAINT public_links_node_fkey FOREIGN KEY (node_id, owner_id)
        REFERENCES nodes (id, owner_id) ON DELETE CASCADE,
    CONSTRAINT public_links_album_fkey FOREIGN KEY (album_id, owner_id)
        REFERENCES albums (id, owner_id) ON DELETE CASCADE
);
-- The links of one item, newest first.
CREATE INDEX public_links_node_idx ON public_links (node_id, created_at DESC)
    WHERE node_id IS NOT NULL;
CREATE INDEX public_links_album_idx ON public_links (album_id, created_at DESC)
    WHERE album_id IS NOT NULL;
-- Account deletion.
CREATE INDEX public_links_owner_idx ON public_links (owner_id);
