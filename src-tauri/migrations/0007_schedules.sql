-- Scheduled prompts (`orchestration.schedules`): open a new session in a project and
-- send it a prompt on a timer, only while the app is open (`schedules::runner`).
--
-- `timing` is JSON read through `schedules::Timing`, in the machine's local time:
--   {"kind":"interval","every_minutes":30}
--   {"kind":"daily","time":"08:30"}
--   {"kind":"weekly","weekdays":[1,3,5],"time":"08:30"}      (0 = Sunday)
--   {"kind":"once","at":"2026-11-09T08:30"}
-- A row whose timing does not parse never comes due.
CREATE TABLE schedules (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    prompt TEXT NOT NULL,
    engine TEXT,                                    -- NULL: the default engine when it fires
    permission_mode TEXT NOT NULL DEFAULT 'plan',
    timing TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    proposed INTEGER NOT NULL DEFAULT 0,            -- 1: written by an agent, not reviewed yet; never runs
    next_run_at TEXT,                               -- RFC 3339 UTC; NULL while off or when nothing comes next
    last_run_at TEXT,
    last_run_id TEXT,                               -- the session the last firing opened
    last_status TEXT,                               -- 'started' | 'skipped' | 'error'
    last_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_schedules_project ON schedules(project_id);
