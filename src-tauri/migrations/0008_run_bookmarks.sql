-- Bookmarks the user put on the turns of a run (`runs::bookmarks`).
--
-- A turn is anchored by its prompt record in the run log: `log_offset` is the byte
-- offset that record starts at (the cursor the viewer jumps to, like ⌘F hits and
-- the prompt list) and `label` its one-line preview. The label lets a log rebuilt
-- from the engine transcript, where offsets moved, be matched again. A Rewind that
-- cuts the log drops the bookmarks of the cut part.
CREATE TABLE run_bookmarks (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    log_offset INTEGER NOT NULL,
    side TEXT NOT NULL,             -- 'question' | 'answer'
    label TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    UNIQUE (run_id, log_offset, side)
);

-- A trigger rather than a foreign key, like 0006: it holds on a connection without
-- `foreign_keys` too, and also fires for runs a project's cascade deletes.
CREATE TRIGGER trg_runs_bookmarks_delete AFTER DELETE ON runs BEGIN
    DELETE FROM run_bookmarks WHERE run_id = old.id;
END;
