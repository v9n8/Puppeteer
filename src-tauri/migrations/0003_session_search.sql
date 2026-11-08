-- Cross-session search: a full-text index over what a reader sees of every run log.
--
-- One row per log record that has user or assistant text (tool input / output and thinking are
-- left out), mirrored Claude / Codex sessions included. The secret vault never gets here: its
-- notes are sealed in the Keychain, never in a run log or in SQLite.
--
-- The app fills the index in the background (`commands::session_search`): a run is re-indexed
-- whole whenever its log changes, and existing runs are caught up after startup. Nothing is
-- copied here, so this migration stays instant on a large database.
--
-- The trigram tokenizer matches any substring of 3+ characters, in any script (Japanese has no
-- spaces between words); shorter queries fall back to LIKE on the records table.

CREATE TABLE session_search_records (
    id INTEGER PRIMARY KEY,
    run_id TEXT NOT NULL,
    log_offset INTEGER NOT NULL,  -- byte offset of the record in the run's log: a get_run_log_forward cursor
    body TEXT NOT NULL            -- the record's user / assistant text blocks, joined by a newline
);
CREATE INDEX idx_session_search_records_run ON session_search_records(run_id);

CREATE VIRTUAL TABLE session_search_fts USING fts5(
    body,
    content = 'session_search_records',
    content_rowid = 'id',
    tokenize = 'trigram'
);

CREATE TRIGGER trg_session_search_records_insert AFTER INSERT ON session_search_records BEGIN
    INSERT INTO session_search_fts (rowid, body) VALUES (new.id, new.body);
END;
CREATE TRIGGER trg_session_search_records_delete AFTER DELETE ON session_search_records BEGIN
    INSERT INTO session_search_fts (session_search_fts, rowid, body) VALUES ('delete', old.id, old.body);
END;

-- The log file each run was indexed from, as last seen, so an unchanged log is not read again.
CREATE TABLE session_search_runs (
    run_id TEXT PRIMARY KEY,
    log_path TEXT NOT NULL,
    log_size INTEGER NOT NULL,
    log_mtime_ns INTEGER NOT NULL
);

-- A trigger rather than a foreign key: it holds on a connection without `foreign_keys` too (the
-- sidecar-mcp one), and also fires for runs a project's cascade deletes.
CREATE TRIGGER trg_runs_session_search_delete AFTER DELETE ON runs BEGIN
    DELETE FROM session_search_records WHERE run_id = old.id;
    DELETE FROM session_search_runs WHERE run_id = old.id;
END;
