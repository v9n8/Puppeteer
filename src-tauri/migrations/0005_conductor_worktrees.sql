-- Conductor workers in their own git worktree (`orchestration.conductor_worktree`).
--
-- The worktree a worker runs in, under `<app data>/worktrees/<worker id>`, on
-- branch `puppeteer/worker-<worker id>` (`conductor::worktree::branch_for`).
-- NULL: the worker runs in the project folder (switch off, or not a git
-- project). Kept after the startup sweep removes a cancelled worker's folder,
-- so a resume brings it back at the same path: Claude keys its transcripts by
-- the working directory.
ALTER TABLE runs ADD COLUMN worktree_path TEXT;

-- conductor_sessions.max_workers: the column default 6 (0001_init.sql) is never
-- used. Every insert binds the cap, `conductor::DEFAULT_MAX_WORKERS` (10) when
-- none is given, and that constant is the one source of the default. The table
-- is not rebuilt only to change a default.
