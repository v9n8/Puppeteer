-- Soft delete (trash) for projects and runs.
--
-- `deleted_at` is the RFC 3339 time the row went to the trash; NULL means live.
-- Every query that lists projects or runs filters `deleted_at IS NULL`; a lookup
-- by id does not, so a trashed row can still be opened, restored or purged. A
-- purge is the old hard delete, which also runs the cleanup that a soft delete
-- postpones (logs, audit, links).
--
-- `projects.path` stays UNIQUE: adding a path back restores the trashed project
-- with the same id instead of creating a second row.
ALTER TABLE projects ADD COLUMN deleted_at TEXT;
ALTER TABLE runs ADD COLUMN deleted_at TEXT;

CREATE INDEX idx_projects_deleted_at ON projects(deleted_at);
CREATE INDEX idx_runs_deleted_at ON runs(deleted_at);
