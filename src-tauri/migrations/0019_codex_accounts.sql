-- Codex accounts managed by Puppeteer (UX-39). Each one signs in with
-- `codex login` under its own CODEX_HOME (`home_dir`, inside app data), the way
-- Claude accounts use CLAUDE_CONFIG_DIR. No secret is stored here: the login
-- lives in the account's home, written by Codex itself.
--
-- `is_default` is 0 on every row until the user picks one: with no default the
-- shared `~/.codex` login stays what Codex sessions use, as before this table.
--
-- `projects.codex_account_id` / `runs.codex_account_id`: the account a project
-- is linked to and the one a session ran with. NULL = fall through to the
-- default account, then to `~/.codex`.
CREATE TABLE codex_accounts (
    id              TEXT PRIMARY KEY,
    label           TEXT NOT NULL UNIQUE,
    home_dir        TEXT NOT NULL UNIQUE,
    email           TEXT,
    auth_method     TEXT,
    status          TEXT NOT NULL DEFAULT 'unknown',
    is_default      INTEGER NOT NULL DEFAULT 0,
    last_checked_at TEXT,
    created_at      TEXT NOT NULL
);
ALTER TABLE projects ADD COLUMN codex_account_id TEXT
    REFERENCES codex_accounts(id) ON DELETE SET NULL;
ALTER TABLE runs ADD COLUMN codex_account_id TEXT
    REFERENCES codex_accounts(id) ON DELETE SET NULL;
CREATE INDEX idx_projects_codex_account ON projects(codex_account_id);
CREATE INDEX idx_runs_codex_account ON runs(codex_account_id);
