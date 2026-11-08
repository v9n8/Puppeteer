-- Workspace snapshots taken before each turn, for Rewind (`sessions.rewind`).
--
-- Metadata only: file contents and manifests live under `<app data>/snapshots`,
-- content-addressed and shared by every run (`runs::snapshots`). One row per prompt
-- Puppeteer handed to the run while the switch was on.
CREATE TABLE run_snapshots (
    id INTEGER PRIMARY KEY,
    run_id TEXT NOT NULL,
    turn INTEGER NOT NULL,          -- 1 for the first prompt captured, +1 per prompt after
    created_at TEXT NOT NULL,       -- RFC 3339, UTC, milliseconds: compared as text
    workspace TEXT NOT NULL,        -- the folder captured: the run's launch folder ('' when unknown)
    status TEXT NOT NULL,           -- 'ok' | 'skipped' (not captured, see reason) | 'pruned' (dropped by cleanup)
    reason TEXT,                    -- why skipped: 'too_many_files' | 'too_large' | 'error'
    manifest_sha TEXT,              -- the manifest blob while status = 'ok'
    file_count INTEGER NOT NULL DEFAULT 0,
    total_bytes INTEGER NOT NULL DEFAULT 0,
    UNIQUE (run_id, turn)
);
CREATE INDEX idx_run_snapshots_manifest ON run_snapshots(manifest_sha);

-- A trigger rather than a foreign key, like 0003: it holds on a connection without
-- `foreign_keys` too, and also fires for runs a project's cascade deletes. The blobs
-- nothing points at any more go at the next cleanup.
CREATE TRIGGER trg_runs_snapshots_delete AFTER DELETE ON runs BEGIN
    DELETE FROM run_snapshots WHERE run_id = old.id;
END;
