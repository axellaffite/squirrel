-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 5: faces hold what the worker validates (rows written so far passed
-- the same checks, so validating them is safe).
ALTER TABLE faces
    ADD CONSTRAINT faces_box_check CHECK (
        box_x BETWEEN 0 AND 1 AND box_y BETWEEN 0 AND 1
        AND box_w > 0 AND box_w <= 1 AND box_h > 0 AND box_h <= 1),
    ADD CONSTRAINT faces_score_check CHECK (score BETWEEN 0 AND 1),
    ADD CONSTRAINT faces_landmarks_not_null CHECK (array_position(landmarks, NULL) IS NULL);

COMMENT ON FUNCTION assets_drop_faces() IS
    'Runs in the transaction deleting the asset, which holds the owner''s users row '
    '(quota::lock_owner, FOR NO KEY UPDATE: every node deletion path takes it, and '
    'deleting the user cascades). faces::store_scan holds the same row FOR SHARE and '
    're-checks that the owner still has the asset, so a scan cannot land after the '
    'last asset went. A new deletion path must take the owner lock too.';
