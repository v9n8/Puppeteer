-- Full schema of a fresh database. Squashed from the original migration
-- history; add new migrations as 0002_*.sql onward.

CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    github_owner TEXT,
    github_repo TEXT,
    default_engine TEXT NOT NULL DEFAULT 'claude',
    created_at TEXT NOT NULL
, github_account_id TEXT
    REFERENCES github_accounts(id) ON DELETE SET NULL, gitlab_account_id TEXT
    REFERENCES gitlab_accounts(id) ON DELETE SET NULL, aws_account_id TEXT
    REFERENCES aws_accounts(id) ON DELETE SET NULL, github_project_board_url TEXT, github_project_field_mappings TEXT, position INTEGER NOT NULL DEFAULT 0, claude_account_id TEXT
    REFERENCES claude_accounts(id) ON DELETE SET NULL, azure_account_id TEXT
    REFERENCES azure_accounts(id) ON DELETE SET NULL, github_reviewer_account_id TEXT
    REFERENCES github_accounts(id) ON DELETE SET NULL);
CREATE TABLE project_secrets (
    project_id TEXT PRIMARY KEY,
    pat_keyring_key TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE TABLE skills (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    source_path TEXT NOT NULL,
    updated_at TEXT NOT NULL
, target TEXT NOT NULL DEFAULT 'claude');
CREATE TABLE sync_conflicts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    detected_at TEXT NOT NULL,
    local_hash TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    resolved INTEGER NOT NULL DEFAULT 0, engine TEXT NOT NULL DEFAULT 'claude',
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (skill_id) REFERENCES skills(id) ON DELETE CASCADE
);
CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    type TEXT NOT NULL,
    ref_number INTEGER,
    status TEXT NOT NULL DEFAULT 'fetched',
    engine TEXT NOT NULL DEFAULT 'claude',
    output_path TEXT,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT NOT NULL, repo_id TEXT REFERENCES repos(id) ON DELETE SET NULL, input_path TEXT, session_id TEXT, title TEXT, transcript_path TEXT, transcript_synced_size INTEGER, pinned INTEGER NOT NULL DEFAULT 0, server_id TEXT, deploy_role TEXT, claude_account_id TEXT
    REFERENCES claude_accounts(id) ON DELETE SET NULL, last_activity_at TEXT, role TEXT, conductor_run_id TEXT, protected INTEGER NOT NULL DEFAULT 0, model TEXT, system_instruction TEXT, framing_seeded INTEGER NOT NULL DEFAULT 0, github_account_id TEXT
    REFERENCES github_accounts(id) ON DELETE SET NULL, owner_device TEXT, device_name TEXT, lease_until TEXT, git_head TEXT, git_dirty INTEGER, final_log_size INTEGER, final_transcript_size INTEGER, owner_epoch INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE repos (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    github_owner TEXT,
    github_repo TEXT,
    created_at TEXT NOT NULL, provider TEXT NOT NULL DEFAULT 'github', gitlab_project_path TEXT, gitlab_project_id INTEGER,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE TABLE rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    target TEXT NOT NULL DEFAULT 'both',   -- 'claude' | 'codex' | 'both'
    source_path TEXT NOT NULL,             -- <app_data>/rules/<name>.md
    updated_at TEXT NOT NULL
);
CREATE TABLE project_rules (
    project_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    target TEXT NOT NULL,                   -- applied target snapshot
    synced_hash_claude TEXT,                -- sha256 of .claude/rules/<name>.md (NULL if not applied to claude)
    synced_hash_codex TEXT,                 -- sha256 of body block in AGENTS.md (NULL if not applied to codex)
    applied_at TEXT NOT NULL, manual INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (project_id, rule_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (rule_id) REFERENCES rules(id) ON DELETE CASCADE
);
CREATE TABLE rule_sync_conflicts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    engine TEXT NOT NULL,                   -- 'claude' | 'codex'
    detected_at TEXT NOT NULL,
    local_hash TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    resolved INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (rule_id) REFERENCES rules(id) ON DELETE CASCADE
);
CREATE TABLE "project_skills" (
    project_id TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    target TEXT NOT NULL DEFAULT 'claude',  -- applied target snapshot
    synced_hash_claude TEXT,                -- sha256 of source SKILL.md synced to .claude/skills (NULL if not applied)
    synced_hash_codex TEXT,                 -- sha256 of source SKILL.md synced to .codex/skills (NULL if not applied)
    applied_at TEXT NOT NULL, manual INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (project_id, skill_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (skill_id) REFERENCES skills(id) ON DELETE CASCADE
);
CREATE TABLE github_accounts (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    username TEXT,            -- GitHub login, filled after validation
    scopes TEXT,             -- comma-separated scopes, cached at validation
    created_at TEXT NOT NULL
, email TEXT);
CREATE TABLE run_usage (
    id                      TEXT PRIMARY KEY,
    run_id                  TEXT,
    project_id              TEXT,
    project_name            TEXT,
    engine                  TEXT NOT NULL,
    model                   TEXT,
    input_tokens            INTEGER NOT NULL DEFAULT 0,
    output_tokens           INTEGER NOT NULL DEFAULT 0,
    cache_creation_tokens   INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens       INTEGER NOT NULL DEFAULT 0,
    total_tokens            INTEGER NOT NULL DEFAULT 0,
    cost_usd                REAL,
    cost_estimated          INTEGER NOT NULL DEFAULT 0,
    num_turns               INTEGER,
    duration_ms             INTEGER,
    created_at              TEXT NOT NULL,
    deleted_run             INTEGER NOT NULL DEFAULT 0
, cost_cumulative_usd REAL, claude_account_id TEXT);
CREATE INDEX idx_run_usage_created ON run_usage(created_at);
CREATE INDEX idx_run_usage_project ON run_usage(project_id);
CREATE INDEX idx_run_usage_engine  ON run_usage(engine);
CREATE INDEX idx_run_usage_run     ON run_usage(run_id);
CREATE UNIQUE INDEX idx_runs_project_engine_session
    ON runs(project_id, engine, session_id)
    WHERE session_id IS NOT NULL;
CREATE TABLE deleted_sessions (
    project_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    engine     TEXT,
    deleted_at TEXT NOT NULL,
    PRIMARY KEY (project_id, session_id)
);
CREATE TABLE gitlab_accounts (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    username TEXT,            -- GitLab username, filled after validation
    host TEXT,                -- GitLab host (e.g. gitlab.com or self-hosted)
    email TEXT,               -- git commit identity email
    scopes TEXT,              -- comma-separated scopes, cached at validation
    created_at TEXT NOT NULL
);
CREATE TABLE mcp_servers (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,      -- valid MCP key (a-zA-Z0-9_-)
    description TEXT,
    transport   TEXT NOT NULL,             -- 'stdio' | 'http' | 'sse'
    command     TEXT,                      -- stdio (non-sensitive)
    args        TEXT,                      -- JSON array (stdio)
    url         TEXT,                      -- http/sse
    env_keys    TEXT,                      -- JSON array of env var KEY names (VALUE in Keychain)
    header_keys TEXT,                      -- JSON array of header KEY names (VALUE in Keychain)
    enabled     INTEGER NOT NULL DEFAULT 1,-- global master switch
    created_at  TEXT NOT NULL
);
CREATE TABLE project_mcp_servers (
    project_id TEXT NOT NULL,
    server_id  TEXT NOT NULL,
    enabled_at TEXT NOT NULL,
    PRIMARY KEY (project_id, server_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (server_id)  REFERENCES mcp_servers(id) ON DELETE CASCADE
);
CREATE TABLE servers (
    id               TEXT PRIMARY KEY,           -- UUID v4
    label            TEXT NOT NULL,              -- display name
    host             TEXT NOT NULL,              -- IP or domain
    port             INTEGER NOT NULL DEFAULT 22,-- SSH port
    username         TEXT NOT NULL,              -- SSH user
    auth_method      TEXT NOT NULL,              -- 'agent' | 'key' (never 'password')
    private_key_path TEXT,                       -- required only when auth_method='key'
    tags             TEXT,                       -- CSV for filtering/grouping
    status           TEXT,                       -- last-known: 'online'|'offline'|'unknown'
    last_checked_at  TEXT,                       -- RFC3339, updated after each test
    created_at       TEXT NOT NULL               -- RFC3339
);
CREATE TABLE project_servers (
    project_id TEXT NOT NULL,
    server_id  TEXT NOT NULL,
    role       TEXT NOT NULL DEFAULT 'production', -- deployment role (staging|production|…)
    created_at TEXT NOT NULL,                      -- RFC3339
    PRIMARY KEY (project_id, server_id, role),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (server_id)  REFERENCES servers(id) ON DELETE CASCADE
);
CREATE TABLE aws_accounts (
    id                TEXT PRIMARY KEY,
    label             TEXT NOT NULL,
    auth_method       TEXT NOT NULL,  -- 'keys' | 'profile'
    account_id        TEXT,           -- AWS 12-digit account id from STS
    arn               TEXT,           -- caller ARN from STS
    region            TEXT NOT NULL,
    access_key_id     TEXT,           -- keys auth only; not the secret key
    profile_name      TEXT,           -- profile auth only
    tags              TEXT,
    last_validated_at TEXT,
    created_at        TEXT NOT NULL
);
CREATE TABLE remote_audit (
    id          TEXT PRIMARY KEY,   -- UUID v4
    ts          TEXT NOT NULL,      -- RFC3339
    device_id   TEXT,               -- source Controller device (may be unknown)
    room_id     TEXT,               -- relay room the command arrived on
    run_id      TEXT,               -- affected run, when applicable
    action      TEXT NOT NULL,      -- e.g. respond_permission | start_run | <rejected action>
    result      TEXT NOT NULL,      -- 'accepted' | 'rejected'
    reason      TEXT                -- machine-readable reason (never a secret/plaintext)
);
CREATE INDEX idx_remote_audit_ts ON remote_audit (ts);
CREATE TABLE todos (
    id         TEXT PRIMARY KEY,
    text       TEXT NOT NULL,
    done       INTEGER NOT NULL DEFAULT 0,
    position   INTEGER NOT NULL,
    created_at TEXT NOT NULL
, project_id TEXT, run_id TEXT, due_date TEXT, due_time TEXT, priority INTEGER NOT NULL DEFAULT 2, estimate_min INTEGER, estimate_source TEXT, status TEXT NOT NULL DEFAULT 'todo', snoozed_until TEXT, remind_at TEXT, source_type TEXT, source_ref TEXT, source_url TEXT, started_at TEXT, completed_at TEXT, actual_min INTEGER, ai_plan TEXT, ai_analyzed_at TEXT, updated_at TEXT);
CREATE INDEX idx_todos_position ON todos(position);
CREATE TABLE notes (
    id         TEXT PRIMARY KEY,
    title      TEXT NOT NULL DEFAULT '',
    content    TEXT NOT NULL DEFAULT '',
    project_id TEXT,
    position   INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
, run_id TEXT);
CREATE INDEX idx_notes_position ON notes(position);
CREATE INDEX idx_notes_project ON notes(project_id);
CREATE TABLE google_accounts (
    id         TEXT PRIMARY KEY,
    label      TEXT NOT NULL UNIQUE,
    email      TEXT,
    scope      TEXT,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_projects_position ON projects(position);
CREATE TABLE claude_accounts (
    id              TEXT PRIMARY KEY,
    label           TEXT NOT NULL UNIQUE,
    config_dir      TEXT NOT NULL UNIQUE,
    email           TEXT,
    auth_method     TEXT,
    api_provider    TEXT,
    status          TEXT NOT NULL DEFAULT 'unknown',
    is_default      INTEGER NOT NULL DEFAULT 0,
    last_checked_at TEXT,
    created_at      TEXT NOT NULL
);
CREATE INDEX idx_projects_claude_account ON projects(claude_account_id);
CREATE INDEX idx_runs_claude_account ON runs(claude_account_id);
CREATE INDEX idx_todos_project ON todos(project_id);
CREATE TABLE skill_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    color TEXT,                             -- badge color, NULL = default
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE skill_group_members (
    group_id TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (group_id, skill_id),
    FOREIGN KEY (group_id) REFERENCES skill_groups(id) ON DELETE CASCADE,
    FOREIGN KEY (skill_id) REFERENCES skills(id) ON DELETE CASCADE
);
CREATE INDEX idx_skill_group_members_skill ON skill_group_members(skill_id);
CREATE TABLE project_skill_groups (
    project_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    enabled_at TEXT NOT NULL,
    PRIMARY KEY (project_id, group_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (group_id) REFERENCES skill_groups(id) ON DELETE CASCADE
);
CREATE TABLE rule_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    color TEXT,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE rule_group_members (
    group_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (group_id, rule_id),
    FOREIGN KEY (group_id) REFERENCES rule_groups(id) ON DELETE CASCADE,
    FOREIGN KEY (rule_id) REFERENCES rules(id) ON DELETE CASCADE
);
CREATE INDEX idx_rule_group_members_rule ON rule_group_members(rule_id);
CREATE TABLE project_rule_groups (
    project_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    enabled_at TEXT NOT NULL,
    PRIMARY KEY (project_id, group_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (group_id) REFERENCES rule_groups(id) ON DELETE CASCADE
);
CREATE TABLE conductor_sessions (
    id TEXT PRIMARY KEY,                    -- == the conductor run id
    project_id TEXT NOT NULL,
    goal TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'running', -- running | done | failed | cancelled
    max_workers INTEGER NOT NULL DEFAULT 6,
    created_at TEXT NOT NULL,
    finished_at TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE TABLE conductor_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    ts TEXT NOT NULL,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (session_id) REFERENCES conductor_sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_conductor_events_session ON conductor_events(session_id);
CREATE INDEX idx_runs_conductor ON runs(conductor_run_id);
CREATE TABLE slack_threads (
  id               TEXT PRIMARY KEY,
  title            TEXT NOT NULL DEFAULT '',
  title_custom     INTEGER NOT NULL DEFAULT 0,     -- 1 = user-chosen, kept on refresh / re-import
  project_id       TEXT,
  position         INTEGER NOT NULL DEFAULT 0,     -- ascending; new rows = MIN(position) - 1
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  workspace_domain TEXT,                            -- link host, e.g. malme.slack.com
  team_id          TEXT,                            -- auth.test team_id
  workspace        TEXT,                            -- auth.test team (name)
  channel_id       TEXT,                            -- C… / G…; NULL only for unparsable legacy rows
  channel          TEXT,                            -- channel name WITHOUT '#'
  thread_ts        TEXT,                            -- thread root ts
  thread_url       TEXT,                            -- canonical link (§4)
  fetch_status     TEXT NOT NULL DEFAULT 'pending', -- pending|fetching|translating|ready|error
  fetch_error_code TEXT,                            -- §8.3
  fetch_error      TEXT,                            -- English detail
  fetched_at       TEXT,                            -- RFC3339 of the last successful fetch
  reply_count      INTEGER NOT NULL DEFAULT 0,
  last_reply_ts    TEXT,                            -- newest message ts
  last_seen_ts     TEXT,                            -- newest ts when the user last viewed it
  warnings         TEXT NOT NULL DEFAULT '[]',      -- JSON array of warning codes (§8.4)
  search_text      TEXT NOT NULL DEFAULT '',        -- generated plain transcript (vi + original) for search / preview
  legacy           INTEGER NOT NULL DEFAULT 0,      -- 1 = old zip/markdown snapshot not yet re-fetched
  legacy_content   TEXT                             -- old markdown body; NULL once re-fetched
);
CREATE UNIQUE INDEX idx_slack_threads_key
  ON slack_threads(channel_id, thread_ts) WHERE channel_id IS NOT NULL AND thread_ts IS NOT NULL;
CREATE INDEX idx_slack_threads_position ON slack_threads(position);
CREATE INDEX idx_slack_threads_project  ON slack_threads(project_id);
CREATE INDEX idx_slack_threads_status   ON slack_threads(fetch_status);
CREATE TABLE slack_messages (
  thread_id        TEXT NOT NULL REFERENCES slack_threads(id) ON DELETE CASCADE,
  ts               TEXT NOT NULL,
  seq              INTEGER NOT NULL,                -- 0 = parent, then chronological
  user_id          TEXT,
  bot_id           TEXT,
  author_name      TEXT,                            -- bot username / fallback name
  author_icon_url  TEXT,                            -- bot icon URL
  subtype          TEXT,
  text_raw         TEXT NOT NULL DEFAULT '',        -- Slack mrkdwn as returned
  text_vi          TEXT,                            -- Vietnamese, same mrkdwn syntax; NULL = not translated
  src_hash         TEXT NOT NULL,                   -- sha256 hex of text_raw
  translate_status TEXT NOT NULL DEFAULT 'pending', -- pending|done|skipped|error
  translate_error  TEXT,
  edited_ts        TEXT,
  reactions        TEXT NOT NULL DEFAULT '[]',      -- JSON [{"name":"+1","count":2}]
  file_ids         TEXT NOT NULL DEFAULT '[]',      -- JSON array of slack_files.id, message order
  PRIMARY KEY (thread_id, ts)
);
CREATE INDEX idx_slack_messages_seq ON slack_messages(thread_id, seq);
CREATE TABLE slack_files (
  id              TEXT PRIMARY KEY,
  thread_id       TEXT NOT NULL REFERENCES slack_threads(id) ON DELETE CASCADE,
  slack_file_id   TEXT,                             -- F…; NULL = legacy zip file
  message_ts      TEXT,                             -- first message carrying it; NULL = legacy
  name            TEXT NOT NULL,
  title           TEXT,
  mime            TEXT,
  filetype        TEXT,
  size            INTEGER NOT NULL DEFAULT 0,
  file_path       TEXT,                             -- RELATIVE to <app_data>; NULL until downloaded
  remote_updated  INTEGER,                          -- file.updated (or created), unix seconds
  permalink       TEXT,
  width           INTEGER,
  height          INTEGER,
  download_status TEXT NOT NULL DEFAULT 'pending',  -- done|pending|error|too_large|unavailable
  download_error  TEXT,
  created_at      TEXT NOT NULL
);
CREATE INDEX idx_slack_files_thread ON slack_files(thread_id);
CREATE UNIQUE INDEX idx_slack_files_remote
  ON slack_files(thread_id, slack_file_id) WHERE slack_file_id IS NOT NULL;
CREATE TABLE slack_users (
  team_id      TEXT NOT NULL,
  user_id      TEXT NOT NULL,
  display_name TEXT,
  real_name    TEXT,
  avatar_url   TEXT,                -- profile.image_72
  avatar_path  TEXT,                -- RELATIVE to <app_data>; NULL until downloaded
  is_bot       INTEGER NOT NULL DEFAULT 0,
  fetched_at   TEXT NOT NULL,
  PRIMARY KEY (team_id, user_id)
);
CREATE TABLE slack_channels (
  team_id    TEXT NOT NULL,
  channel_id TEXT NOT NULL,
  name       TEXT,
  kind       TEXT,                  -- public|private|im|mpim|unknown
  fetched_at TEXT NOT NULL,
  PRIMARY KEY (team_id, channel_id)
);
CREATE TABLE slack_legacy_orphans (id TEXT PRIMARY KEY);
CREATE TABLE slack_thread_runs (
  thread_id  TEXT NOT NULL REFERENCES slack_threads(id) ON DELETE CASCADE,
  run_id     TEXT NOT NULL,
  project_id TEXT NOT NULL,
  source     TEXT NOT NULL,          -- manual | picker | created_from_thread
  linked_at  TEXT NOT NULL,          -- RFC3339
  PRIMARY KEY (thread_id, run_id)
);
CREATE INDEX idx_slack_thread_runs_run ON slack_thread_runs(run_id);
CREATE TABLE note_runs (
  note_id    TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  run_id     TEXT NOT NULL,
  project_id TEXT NOT NULL,
  source     TEXT NOT NULL,          -- manual | picker | created_from_item | captured
  linked_at  TEXT NOT NULL,          -- RFC3339
  PRIMARY KEY (note_id, run_id)
);
CREATE INDEX idx_note_runs_run ON note_runs(run_id);
CREATE TABLE todo_runs (
  todo_id    TEXT NOT NULL REFERENCES todos(id) ON DELETE CASCADE,
  run_id     TEXT NOT NULL,
  project_id TEXT NOT NULL,
  source     TEXT NOT NULL,          -- manual | picker | created_from_item | captured
  linked_at  TEXT NOT NULL,          -- RFC3339
  PRIMARY KEY (todo_id, run_id)
);
CREATE INDEX idx_todo_runs_run ON todo_runs(run_id);
CREATE TABLE run_usage_baseline (
    run_id       TEXT PRIMARY KEY,
    model_usage  TEXT NOT NULL,
    carries_over INTEGER NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE TABLE azure_accounts (
    id                TEXT PRIMARY KEY,
    label             TEXT NOT NULL,
    tenant            TEXT,           -- user input for `az login --tenant` (id or domain), optional
    tenant_id         TEXT,           -- resolved tenant id from `az account show`
    subscription_id   TEXT,           -- subscription pinned in the account dir
    subscription_name TEXT,
    user_name         TEXT,           -- signed-in user / service principal name
    tags              TEXT,
    last_validated_at TEXT,
    created_at        TEXT NOT NULL
);
CREATE TABLE slack_identities (
  team_id    TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL,
  fetched_at TEXT NOT NULL
);
CREATE INDEX idx_runs_running ON runs(status) WHERE status = 'running';
CREATE INDEX idx_run_usage_claude_account ON run_usage(claude_account_id);
CREATE TRIGGER trg_run_usage_claude_account
AFTER INSERT ON run_usage
WHEN NEW.engine = 'claude' AND NEW.claude_account_id IS NULL AND NEW.run_id IS NOT NULL
BEGIN
    UPDATE run_usage
       SET claude_account_id = (SELECT r.claude_account_id FROM runs r WHERE r.id = NEW.run_id)
     WHERE id = NEW.id;
END;
CREATE INDEX idx_todos_due ON todos(done, due_date);
CREATE TABLE todo_notifications (
    todo_id  TEXT NOT NULL,
    kind     TEXT NOT NULL,
    fire_key TEXT NOT NULL,
    fired_at TEXT NOT NULL,
    PRIMARY KEY (todo_id, kind, fire_key)
);
CREATE TABLE focus_day_plans (
    date       TEXT PRIMARY KEY,   -- YYYY-MM-DD
    plan_json  TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE remote_push_subscriptions (
    id            TEXT PRIMARY KEY,
    endpoint      TEXT NOT NULL UNIQUE,
    p256dh        TEXT NOT NULL,
    auth          TEXT NOT NULL,
    label         TEXT NOT NULL DEFAULT '',
    events        TEXT NOT NULL DEFAULT 'pending,done,failed',
    vapid_key     TEXT NOT NULL DEFAULT '',
    session_tag   TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL,
    last_ok_at    TEXT,
    removed_at    TEXT,
    auth_failures INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE project_github_accounts (
    project_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    created_at TEXT NOT NULL,                   -- RFC3339
    PRIMARY KEY (project_id, account_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (account_id) REFERENCES github_accounts(id) ON DELETE CASCADE
);
CREATE TABLE project_gitlab_accounts (
    project_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    created_at TEXT NOT NULL,                   -- RFC3339
    PRIMARY KEY (project_id, account_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (account_id) REFERENCES gitlab_accounts(id) ON DELETE CASCADE
);
CREATE TABLE project_aws_accounts (
    project_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    created_at TEXT NOT NULL,                   -- RFC3339
    PRIMARY KEY (project_id, account_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (account_id) REFERENCES aws_accounts(id) ON DELETE CASCADE
);
CREATE TABLE project_azure_accounts (
    project_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    created_at TEXT NOT NULL,                   -- RFC3339
    PRIMARY KEY (project_id, account_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (account_id) REFERENCES azure_accounts(id) ON DELETE CASCADE
);
CREATE TABLE project_builtin_mcp_disabled (
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,                         -- `puppeteer` | `google`
    created_at TEXT NOT NULL,                   -- RFC3339
    PRIMARY KEY (project_id, name),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

INSERT INTO settings VALUES('default_engine','claude');
INSERT INTO settings VALUES('claude_path','claude');
INSERT INTO settings VALUES('codex_path','codex');
INSERT INTO settings VALUES('extra_args','');
INSERT INTO settings VALUES('theme','system');
INSERT INTO settings VALUES('analyze_issue_prompt','Please analyze the GitHub issue described in the file and create a detailed implementation plan.');
INSERT INTO settings VALUES('review_pr_prompt','Please review the pull request described in the file according to the configured skills.');
