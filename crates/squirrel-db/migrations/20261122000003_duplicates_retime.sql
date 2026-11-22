-- FROZEN once merged: never edit; add a new migration instead.
-- Duplicates: a visible photo whose capture time changes leaves its group and
-- is looked at again (the 24 h window is re-checked). Hidden photos stay put.

CREATE FUNCTION assets_retime_leaves_duplicates() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM duplicate_members WHERE node_id = NEW.node_id;
    UPDATE assets SET duplicates_checked = FALSE
     WHERE node_id = NEW.node_id AND duplicates_checked;
    RETURN NULL;
END;
$$;
CREATE TRIGGER assets_retime_leaves_duplicates AFTER UPDATE OF taken_at ON assets
    FOR EACH ROW WHEN (OLD.taken_at IS DISTINCT FROM NEW.taken_at AND NEW.hidden_at IS NULL)
    EXECUTE FUNCTION assets_retime_leaves_duplicates();
