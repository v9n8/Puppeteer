-- A project's standing tool permissions (`commands::tool_permissions`, UX-31): the
-- "Always allow" / "Always deny" answers of a permission prompt, kept per project so
-- they apply again in every later session. Before this table they lived in the
-- webview's localStorage only (no backup, lost with the webview data); the app copies
-- them in once on first start.
--
-- `tool_name` is matched exactly: an MCP tool is the full `mcp__<server>__<tool>`.
-- `created_at` is when the current decision was set (a flip resets it), so the
-- lists show the latest choices last, as before.
CREATE TABLE project_tool_permissions (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    tool_name TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('allow', 'deny')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (project_id, tool_name)
);

-- Like 0006 / 0008 / 0012: also holds on a connection without `foreign_keys`.
CREATE TRIGGER trg_projects_tool_permissions_delete AFTER DELETE ON projects BEGIN
    DELETE FROM project_tool_permissions WHERE project_id = old.id;
END;
