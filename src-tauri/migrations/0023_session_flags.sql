-- Session flags (`commands::session_flags`, V3-18): one coloured label per session
-- ("Needs review", "In progress", …) shown on the History rows and used to filter them.
--
-- `project_id` NULL is a shared flag, usable in every project; otherwise the flag is that
-- project's own. Names are unique per scope, case-insensitively: NULL is spelled '' in the
-- index because two NULLs never clash in a UNIQUE constraint (like 0011). `color` is a
-- design token of the project tab palette (`projects::PROJECT_COLORS`), never a raw colour,
-- so it follows the light / dark theme.
--
-- The four default flags are rows like the user's, with their key as id and `builtin_key`:
-- the user renames or deletes them, `restore_default_session_flags` puts back the deleted
-- ones, and while a default keeps its seeded English name the UI shows it translated.
-- The seed must match `session_flags::DEFAULT_FLAGS` (a test checks it).
CREATE TABLE session_flags (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    color TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    builtin_key TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_session_flags_scope_name
    ON session_flags (IFNULL(project_id, ''), name COLLATE NOCASE);
CREATE UNIQUE INDEX idx_session_flags_builtin_key
    ON session_flags (builtin_key) WHERE builtin_key IS NOT NULL;
CREATE INDEX idx_session_flags_scope_position ON session_flags (project_id, position);

-- A project hides a shared flag from its sessions' menus. Only shared flags are hidden
-- this way; a session that already has the flag keeps it.
CREATE TABLE project_hidden_flags (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    flag_id TEXT NOT NULL REFERENCES session_flags(id) ON DELETE CASCADE,
    PRIMARY KEY (project_id, flag_id)
);

-- The session's flag; NULL is "No flag", which every session from before this column has.
ALTER TABLE runs ADD COLUMN flag_id TEXT;
CREATE INDEX idx_runs_flag ON runs (flag_id) WHERE flag_id IS NOT NULL;

-- A trigger rather than a foreign key, like 0008: it holds on a connection without
-- `foreign_keys` too, and also fires for a project's own flags its cascade deletes.
CREATE TRIGGER trg_session_flags_delete AFTER DELETE ON session_flags BEGIN
    UPDATE runs SET flag_id = NULL WHERE flag_id = old.id;
    DELETE FROM project_hidden_flags WHERE flag_id = old.id;
END;

INSERT INTO session_flags (id, project_id, name, color, position, builtin_key, created_at, updated_at) VALUES
    ('needs_review', NULL, 'Needs review', 'warning', 0, 'needs_review',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('in_progress', NULL, 'In progress', 'info', 1, 'in_progress',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('awaiting_review', NULL, 'Awaiting review', 'primary', 2, 'awaiting_review',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ('settled', NULL, 'Settled', 'success', 3, 'settled',
     strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
