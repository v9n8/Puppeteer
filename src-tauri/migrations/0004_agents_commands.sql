-- Agents and Commands libraries: Claude Code subagents and slash commands, kept like skills.
--
-- Each item is one markdown file in the library (<app data>/agents/<name>.md,
-- <app data>/commands/<name>.md). A project gets it as a symlink in `.claude/agents/<name>.md` or
-- `.claude/commands/<name>.md`, so editing the source updates every project. Claude only: Codex
-- has no per-project folder for either, so there are no per-engine switches; a row means the item
-- is switched on for Claude in that project.

CREATE TABLE agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    source_path TEXT NOT NULL,              -- <app data>/agents/<name>.md
    updated_at TEXT NOT NULL
);

CREATE TABLE commands (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    source_path TEXT NOT NULL,              -- <app data>/commands/<name>.md
    updated_at TEXT NOT NULL
);

CREATE TABLE project_agents (
    project_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    synced_hash_claude TEXT,  -- sha256 of the source linked into .claude/agents (NULL: not installed; a path git tracks is the repo's own and stays NULL)
    applied_at TEXT NOT NULL,
    PRIMARY KEY (project_id, agent_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE TABLE project_commands (
    project_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    synced_hash_claude TEXT,  -- sha256 of the source linked into .claude/commands (NULL: same as above)
    applied_at TEXT NOT NULL,
    PRIMARY KEY (project_id, command_id),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (command_id) REFERENCES commands(id) ON DELETE CASCADE
);
