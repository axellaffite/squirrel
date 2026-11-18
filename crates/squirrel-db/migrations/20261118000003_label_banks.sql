-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 5: which label bank content (labels file, prompt templates, background
-- prompts) each stored (model, labels version) bank was computed from. A bank
-- whose digest differs from the code's is ignored and computed again.
CREATE TABLE clip_label_banks (
    model          TEXT NOT NULL CHECK (char_length(model) <= 64),
    labels_version SMALLINT NOT NULL,
    digest         TEXT NOT NULL CHECK (digest ~ '^[0-9a-f]{64}$'),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (model, labels_version)
);
