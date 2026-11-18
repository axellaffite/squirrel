-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 5: when clustering last left an unassigned face waiting, as the
-- owner's newest face time at that run. The face is tried again only once a
-- newer face exists, so waiting faces never starve new ones.
ALTER TABLE faces ADD COLUMN cluster_tried_at TIMESTAMPTZ;
