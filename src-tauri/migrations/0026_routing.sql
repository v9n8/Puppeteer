-- Model / effort routing (QO-06, `runs::routing`).
--
-- routing_policies: every version of the routing policy a new Claude session
-- gets appended to its system prompt. The full text is stored, not only the
-- table it was built from, so a resume sends the very bytes its session started
-- with even after the text template changes (AC-06). The current version is
-- MAX(version); version 1 is written at startup (`policy::ensure_seeded`), from
-- the same code that builds every other version.
--   source: 'default' (built in) | 'auto' (recomputed) | 'revert'.
--   changes_json: what changed from the previous version, with the counts behind it.
CREATE TABLE routing_policies (
    version      INTEGER PRIMARY KEY,
    created_at   TEXT NOT NULL,
    source       TEXT NOT NULL,
    table_json   TEXT NOT NULL,
    text         TEXT NOT NULL,
    changes_json TEXT NOT NULL DEFAULT '[]'
);

-- routing_decisions: one row per delegated piece of work — a main-thread
-- `Agent` call read from the stream (kind 'subagent') or a `session_spawn`
-- (kind 'worker') — and how it ended. Read only by the routing report and the
-- policy update; no model is ever called for them.
--
-- No foreign key to runs, like run_usage: a run purged from the Trash keeps its
-- rows for the statistics. Never holds a prompt or a sub-agent's output; the
-- description is cut to 80 characters.
--   run_id: the run whose agent decided (a worker row: its conductor).
--   decided_by: 'agent' (it named a model or effort) | 'default' | 'escalation'.
--   effort_source: 'param' | 'env' | 'worker_setting' | 'unknown'.
--   policy_version: the deciding session's (NULL: none bound, 0: none on purpose).
--   status: NULL while open, then 'completed' | 'failed' | 'stopped'.
--   verification: a worker's last `Verification:` line, 'pass' | 'fail' | 'none' | 'missing'.
--   escalated: 1 once a later call of the same work went one tier up.
--   clamp: 'from→to' when the app lowered the requested model to the session's.
CREATE TABLE routing_decisions (
    id               TEXT PRIMARY KEY,
    run_id           TEXT,
    project_id       TEXT,
    created_at       TEXT NOT NULL,
    finished_at      TEXT,
    kind             TEXT NOT NULL,
    decided_by       TEXT NOT NULL,
    agent_type       TEXT,
    task_kind        TEXT NOT NULL DEFAULT 'other',
    requested_model  TEXT,
    resolved_model   TEXT,
    requested_effort TEXT,
    effort_source    TEXT NOT NULL DEFAULT 'unknown',
    policy_version   INTEGER,
    description      TEXT,
    tool_use_id      TEXT,
    worker_run_id    TEXT,
    status           TEXT,
    verification     TEXT,
    total_tokens     INTEGER,
    duration_ms      INTEGER,
    escalated        INTEGER NOT NULL DEFAULT 0,
    clamp            TEXT
);
CREATE INDEX idx_routing_decisions_created ON routing_decisions(created_at);
CREATE INDEX idx_routing_decisions_run ON routing_decisions(run_id);
CREATE INDEX idx_routing_decisions_tool_use ON routing_decisions(tool_use_id);

-- The policy version a run was bound to at its first start: NULL = never bound
-- (a run from before 0026, or not started yet), 0 = started without a policy
-- (switched off, Codex, or a worker of such a conductor), N = version N.
ALTER TABLE runs ADD COLUMN routing_policy_version INTEGER;
