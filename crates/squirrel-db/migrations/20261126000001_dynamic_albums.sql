-- FROZEN once merged: never edit; add a new migration instead.
-- Dynamic albums: an album with a rule row holds no items of its own; its
-- photos are the owner's live photos matching the rule, computed on read.
-- Groups combine with AND; empty arrays and NULL columns are "any".

CREATE TABLE album_rules (
    album_id      UUID PRIMARY KEY,
    owner_id      UUID NOT NULL,
    -- The owner's people. No foreign key: a person deleted since matches
    -- nothing (the album empties instead of silently widening); merges
    -- rewrite the ids (squirrel_db::people::merge).
    people        UUID[] NOT NULL DEFAULT '{}' CHECK (cardinality(people) <= 20),
    people_match  TEXT NOT NULL DEFAULT 'any' CHECK (people_match IN ('any', 'all')),
    tags          TEXT[] NOT NULL DEFAULT '{}' CHECK (cardinality(tags) <= 20),
    tags_match    TEXT NOT NULL DEFAULT 'any' CHECK (tags_match IN ('any', 'all')),
    country       TEXT CHECK (char_length(country) BETWEEN 1 AND 200),
    -- '' = places without a region, as in photo filters.
    region        TEXT CHECK (char_length(region) <= 200),
    city          TEXT CHECK (char_length(city) BETWEEN 1 AND 200),
    camera_make   TEXT CHECK (char_length(camera_make) BETWEEN 1 AND 200),
    camera_model  TEXT CHECK (char_length(camera_model) BETWEEN 1 AND 200),
    kind          TEXT CHECK (kind IN ('photo', 'screenshot')),
    -- Wall-clock capture dates, both inclusive.
    taken_from    DATE,
    taken_to      DATE,
    CONSTRAINT album_rules_dates CHECK (taken_from IS NULL OR taken_to IS NULL
                                        OR taken_from <= taken_to),
    CONSTRAINT album_rules_album_fkey FOREIGN KEY (album_id, owner_id)
        REFERENCES albums (id, owner_id) ON DELETE CASCADE
);
-- Person merges and "albums of this photo".
CREATE INDEX album_rules_owner_idx ON album_rules (owner_id);
