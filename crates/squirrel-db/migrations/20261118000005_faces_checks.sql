-- FROZEN once merged: never edit; add a new migration instead.
-- Phase 5: faces hold what the worker validates (rows written so far passed
-- the same checks, so validating them is safe).
ALTER TABLE faces
    ADD CONSTRAINT faces_box_check CHECK (
        box_x BETWEEN 0 AND 1 AND box_y BETWEEN 0 AND 1
        AND box_w > 0 AND box_w <= 1 AND box_h > 0 AND box_h <= 1),
    ADD CONSTRAINT faces_score_check CHECK (score BETWEEN 0 AND 1),
    ADD CONSTRAINT faces_landmarks_not_null CHECK (array_position(landmarks, NULL) IS NULL);
