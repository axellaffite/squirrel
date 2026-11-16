-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 3: albums, per user, holding timeline assets.

CREATE TABLE albums (
    id             UUID PRIMARY KEY,
    owner_id       UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name           TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    -- Chosen cover; NULL means "the most recent item".
    cover_node_id  UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT albums_id_owner_key UNIQUE (id, owner_id),
    -- Postgres >= 15: SET NULL on the cover column only (owner_id stays).
    CONSTRAINT albums_cover_fkey FOREIGN KEY (cover_node_id, owner_id)
        REFERENCES assets (node_id, owner_id) ON DELETE SET NULL (cover_node_id)
);
CREATE INDEX albums_owner_idx ON albums (owner_id, updated_at DESC, id DESC);

CREATE TABLE album_items (
    album_id  UUID NOT NULL,
    node_id   UUID NOT NULL,
    owner_id  UUID NOT NULL,
    added_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (album_id, node_id),
    -- Both sides belong to the same owner: an album never holds another user's photo.
    CONSTRAINT album_items_album_fkey FOREIGN KEY (album_id, owner_id)
        REFERENCES albums (id, owner_id) ON DELETE CASCADE,
    CONSTRAINT album_items_asset_fkey FOREIGN KEY (node_id, owner_id)
        REFERENCES assets (node_id, owner_id) ON DELETE CASCADE
);
CREATE INDEX album_items_node_idx ON album_items (node_id);
