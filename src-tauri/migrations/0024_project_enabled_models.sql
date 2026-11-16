-- The models a project's sessions may use (Project › Model, V3-64; `model_policy::project`).
-- Same JSON as the `enabled_models` setting (V3-62): `{"claude": [id…] | null, "codex": [id…] | null}`.
-- NULL = the project follows Settings › Model, which is what every existing project does.
-- A run checks the intersection of the two lists, so a model turned off in Settings is off
-- here too without touching this column, and comes back once it is turned on again.
ALTER TABLE projects ADD COLUMN enabled_models TEXT;
