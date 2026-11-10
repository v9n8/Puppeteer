-- Response styles the user writes (`commands::response_styles`): a short text added to
-- the system prompt of a session so the answers follow a tone or format.
--
-- `project_id` NULL is a global style, usable in every project; otherwise the style is
-- usable only in that project. The built-in styles (Default, Concise, …) live in code and
-- are not rows. Names are unique per scope, case-insensitively: NULL is spelled '' in the
-- index because two NULLs never clash in a UNIQUE constraint.
CREATE TABLE response_styles (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_response_styles_scope_name
    ON response_styles (IFNULL(project_id, ''), name COLLATE NOCASE);

-- The style a session runs with: a built-in key or a `response_styles.id`, kept so a
-- resume uses it again. NULL (sessions from before this column) falls back to the
-- default style setting.
ALTER TABLE runs ADD COLUMN style TEXT;
