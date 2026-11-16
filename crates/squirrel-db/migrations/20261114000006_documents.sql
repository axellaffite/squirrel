-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 3: full-text search over PDF and plain-text documents, and file names.

-- `unaccent` is a trusted extension (Postgres >= 13): the database owner may create it.
CREATE EXTENSION IF NOT EXISTS unaccent;

-- Language-agnostic: no stemming (French and English mix in one home), accents
-- folded, so "ecole" finds "École". Queries add prefix matching.
CREATE TEXT SEARCH CONFIGURATION squirrel_text (COPY = pg_catalog.simple);
ALTER TEXT SEARCH CONFIGURATION squirrel_text
    ALTER MAPPING FOR hword, hword_part, word WITH unaccent, simple;

-- Text is indexed as `to_tsvector('squirrel_text', translate(text, '._-/', '    '))`
-- (names and bodies alike): the default parser would otherwise keep
-- "Rapport_2026.pdf" or "2026-01-15" as single tokens no word query matches.
CREATE TABLE document_text (
    sha256      TEXT PRIMARY KEY REFERENCES media (sha256) ON DELETE CASCADE,
    -- Extracted text, capped at 256 KiB (kept for result snippets).
    body        TEXT NOT NULL CHECK (octet_length(body) <= 262144),
    content     TSVECTOR NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX document_text_content_idx ON document_text USING GIN (content);

-- Name search over live files (same expression as the queries, or it is not used).
CREATE INDEX nodes_name_search_idx
    ON nodes USING GIN (to_tsvector('squirrel_text', translate(name, '._-/', '    ')))
    WHERE kind = 'file' AND deleted_at IS NULL;
