-- MCP servers one session switched on or off against its project (`commands::mcp`, UX-27).
--
-- A session gets its project's MCP set (`project_mcp_servers`, `project_builtin_mcp_disabled`)
-- and only the difference is stored here, so a project change still reaches every session
-- that has not overridden that server. `server_key` is `server:<mcp_servers.id>` for a
-- user server or `builtin:<name>` for a built-in one (`puppeteer`, `google`).
--
-- Sessions are not in backups, so these rows are not either.
CREATE TABLE run_mcp_overrides (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    server_key TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (run_id, server_key)
);

-- Like 0006 / 0008: also holds on a connection without `foreign_keys`.
CREATE TRIGGER trg_runs_mcp_overrides_delete AFTER DELETE ON runs BEGIN
    DELETE FROM run_mcp_overrides WHERE run_id = old.id;
END;

-- A deleted server leaves no override behind.
CREATE TRIGGER trg_mcp_servers_overrides_delete AFTER DELETE ON mcp_servers BEGIN
    DELETE FROM run_mcp_overrides WHERE server_key = 'server:' || old.id;
END;
