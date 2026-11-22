-- FROZEN once merged: never edit; add a new migration instead.
-- Duplicates (20261122000001 is frozen): a photo leaving its group locks the
-- group before counting what is left, so a grouping run adding a photo to it
-- at that moment is waited for and counted (the group is not deleted under
-- the new photo).

CREATE OR REPLACE FUNCTION duplicate_members_settle() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM 1 FROM duplicate_groups WHERE id = OLD.group_id FOR UPDATE;
    UPDATE duplicate_groups SET kept_node_id = NULL, updated_at = now()
     WHERE id = OLD.group_id AND kept_node_id = OLD.node_id;
    DELETE FROM duplicate_groups g
     WHERE g.id = OLD.group_id
       AND NOT EXISTS (SELECT 1 FROM duplicate_members m
                         JOIN assets a ON a.node_id = m.node_id
                        WHERE m.group_id = g.id
                        GROUP BY m.group_id
                       HAVING count(*) >= 2 OR bool_or(a.hidden_at IS NOT NULL));
    RETURN NULL;
END;
$$;
