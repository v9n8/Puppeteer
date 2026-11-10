-- Per-project defaults for new sessions (UX-19, `commands::session_defaults`). Each column
-- NULL = follow the global setting of the same meaning (Settings › Defaults for new
-- sessions). A run takes, field by field: its own pick > the run's own stored value (a
-- resume keeps it) > these > the global setting.
--
-- The engine gets a new column instead of reusing `projects.default_engine` (0001): that
-- column is `NOT NULL DEFAULT 'claude'` and nothing ever read or set it, so every project
-- holds 'claude' whether or not the user chose it, and every INSERT that leaves it out
-- (add_project, …) writes 'claude' again. Reading it would pin every project to Claude
-- over a global Codex default. It stays as is (backups still carry it).
ALTER TABLE projects ADD COLUMN default_engine_override TEXT;
-- The default model of each engine, like the global `claude_model` / `codex_model`.
ALTER TABLE projects ADD COLUMN default_model_claude TEXT;
ALTER TABLE projects ADD COLUMN default_model_codex TEXT;
ALTER TABLE projects ADD COLUMN default_permission_mode TEXT;
ALTER TABLE projects ADD COLUMN default_effort TEXT;
-- A built-in style key or a `response_styles.id` (global, or this project's own).
ALTER TABLE projects ADD COLUMN default_style TEXT;

-- The reasoning effort a run last ran with, kept so a resume uses it again (UX-16). NULL:
-- the engine's own default, or a run from before this column (the defaults then apply).
ALTER TABLE runs ADD COLUMN effort TEXT;
