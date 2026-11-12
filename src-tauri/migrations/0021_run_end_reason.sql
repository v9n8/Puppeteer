-- Why a run ended when the user did not end it (HD-04). A run cut off by the app
-- exiting used to become a plain 'cancelled', the same as one the user stopped,
-- so nobody could tell which sessions to pick up again.
--
-- 'app_closed': still running when the app last exited, crash or normal quit
-- alike (db::reconcile_interrupted_runs). 'quota': stopped by runs::quota_stop
-- (HD-07). NULL otherwise. Cleared when the run starts a turn again.
-- Sessions are not in backups (0014), so neither are these.
ALTER TABLE runs ADD COLUMN end_reason TEXT;
-- 1 while an 'app_closed' run still waits for the user to resume or dismiss it.
-- conductor::worktree::sweep keeps a pending worker's worktree, with its
-- uncommitted edits, until then.
ALTER TABLE runs ADD COLUMN interrupted_pending INTEGER NOT NULL DEFAULT 0;
