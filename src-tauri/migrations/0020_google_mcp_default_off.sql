-- The built-in `google` MCP server is now off until the user switches it on
-- (UX-78): it reads and sends the user's mail and files, so it waits for an
-- explicit yes. `builtin_mcp_globally_enabled` reads an unset
-- `mcp_builtin_google_enabled` as off from here on. A database that already
-- has a Google account connected was getting the server, so it keeps it.
INSERT OR IGNORE INTO settings (key, value)
SELECT 'mcp_builtin_google_enabled', 'true'
WHERE EXISTS (SELECT 1 FROM google_accounts);
