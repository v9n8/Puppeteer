-- Fork (`runs::fork`, UX-28): the session a run was forked from, and when.
--
-- A fork is a session of its own: deleting either one leaves the other as it is.
-- Deleting the source only drops the link (the trigger, rather than a foreign key,
-- also holds on a connection without `foreign_keys`, like 0006 / 0008 / 0012).
-- Sessions are not in backups, so neither is this.
ALTER TABLE runs ADD COLUMN forked_from_run_id TEXT;
ALTER TABLE runs ADD COLUMN forked_at TEXT;

CREATE TRIGGER trg_runs_forks_unlink AFTER DELETE ON runs BEGIN
    UPDATE runs SET forked_from_run_id = NULL WHERE forked_from_run_id = old.id;
END;
