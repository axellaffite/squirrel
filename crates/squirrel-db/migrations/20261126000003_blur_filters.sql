-- FROZEN once merged: never edit; add a new migration instead.
-- A user's blur filter: their own photos matching it are shown blurred by
-- every client until revealed. Same rule columns (and meaning) as
-- album_rules, so squirrel_db::album_rules::RULE_MATCHES reads it as `r`.
-- One per user; `enabled` turns it off without forgetting the rule.

CREATE TABLE blur_filters (
    owner_id      UUID PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    enabled       BOOLEAN NOT NULL DEFAULT TRUE,
    -- The owner's people, without a foreign key (as album_rules).
    people        UUID[] NOT NULL DEFAULT '{}' CHECK (cardinality(people) <= 20),
    people_match  TEXT NOT NULL DEFAULT 'any' CHECK (people_match IN ('any', 'all')),
    tags          TEXT[] NOT NULL DEFAULT '{}' CHECK (cardinality(tags) <= 20),
    tags_match    TEXT NOT NULL DEFAULT 'any' CHECK (tags_match IN ('any', 'all')),
    country       TEXT CHECK (char_length(country) BETWEEN 1 AND 200),
    region        TEXT CHECK (char_length(region) <= 200),
    city          TEXT CHECK (char_length(city) BETWEEN 1 AND 200),
    camera_make   TEXT CHECK (char_length(camera_make) BETWEEN 1 AND 200),
    camera_model  TEXT CHECK (char_length(camera_model) BETWEEN 1 AND 200),
    kind          TEXT CHECK (kind IN ('photo', 'screenshot')),
    taken_from    DATE,
    taken_to      DATE,
    search              TEXT CHECK (char_length(search) BETWEEN 1 AND 200),
    search_model        TEXT CHECK (char_length(search_model) BETWEEN 1 AND 64),
    search_embedding    halfvec(768),
    search_rivals       halfvec(768)[] NOT NULL DEFAULT '{}'
        CHECK (cardinality(search_rivals) <= 64),
    search_max_distance DOUBLE PRECISION,
    search_rival_margin DOUBLE PRECISION,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT blur_filters_dates CHECK (taken_from IS NULL OR taken_to IS NULL
                                         OR taken_from <= taken_to),
    CONSTRAINT blur_filters_search CHECK (
        (search IS NULL) = (search_model IS NULL)
        AND (search IS NULL) = (search_embedding IS NULL)
        AND (search IS NULL) = (search_max_distance IS NULL)
        AND (search IS NULL) = (search_rival_margin IS NULL))
);
