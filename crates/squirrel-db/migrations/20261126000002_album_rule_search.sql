-- FROZEN once merged: never edit; add a new migration instead.
-- A dynamic album's rule may also hold a search text: its photos must then
-- match it as photo search would. The API encodes the text when the rule is
-- saved and stores what matching needs, so reads never run the text model:
-- the text's embedding (of `search_model`), the label embeddings it must beat
-- on a photo (search's rivals: "dog" against "cat"), and search's thresholds
-- at that time, as cosine distances.

ALTER TABLE album_rules
    ADD COLUMN search              TEXT CHECK (char_length(search) BETWEEN 1 AND 200),
    ADD COLUMN search_model        TEXT CHECK (char_length(search_model) BETWEEN 1 AND 64),
    ADD COLUMN search_embedding    halfvec(768),
    ADD COLUMN search_rivals       halfvec(768)[] NOT NULL DEFAULT '{}'
        CHECK (cardinality(search_rivals) <= 64),
    ADD COLUMN search_max_distance DOUBLE PRECISION,
    ADD COLUMN search_rival_margin DOUBLE PRECISION,
    ADD CONSTRAINT album_rules_search CHECK (
        (search IS NULL) = (search_model IS NULL)
        AND (search IS NULL) = (search_embedding IS NULL)
        AND (search IS NULL) = (search_max_distance IS NULL)
        AND (search IS NULL) = (search_rival_margin IS NULL));
