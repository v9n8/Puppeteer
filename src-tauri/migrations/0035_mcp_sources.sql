-- Where a managed MCP server came from, so discovery can tell "Not managed" / "Managed" /
-- "Changed since import" and Sync can refresh the copy from its source (MCP connection control).
-- All NULL for a server created in Puppeteer. Only names and paths: never an env / header value.
--   source_kind        'S2'..'S9' (the requirements' source ids) or 'claudeai' (a claude.ai connector)
--   source_ref         canonical source file path, or a project id; for 'claudeai' the profile
--   source_name        the server's key in that source (a connector's claude.ai name, e.g. 'claude.ai Slack')
--   source_fingerprint sha256 of the source entry at import / sync; read by the backend only
--   connector_account  'claudeai' rows only: the Claude profile, a claude_accounts.id or 'global'
ALTER TABLE mcp_servers ADD COLUMN source_kind TEXT;
ALTER TABLE mcp_servers ADD COLUMN source_ref TEXT;
ALTER TABLE mcp_servers ADD COLUMN source_name TEXT;
ALTER TABLE mcp_servers ADD COLUMN source_fingerprint TEXT;
ALTER TABLE mcp_servers ADD COLUMN connector_account TEXT;

-- The claude.ai connectors each Claude profile reported at its last probe. A profile is an app
-- account (claude_accounts.id) or 'global' (not a row, hence no foreign key). Re-probed, never
-- backed up. `url` / `connector_id` stay in the backend: a run passes them back to the engine.
CREATE TABLE claude_connectors (
    account_ref  TEXT NOT NULL,
    name         TEXT NOT NULL,
    connector_id TEXT NOT NULL,
    url          TEXT NOT NULL,
    status       TEXT NOT NULL,
    seen_at      TEXT NOT NULL,
    PRIMARY KEY (account_ref, name)
);

CREATE TRIGGER claude_connectors_account_deleted AFTER DELETE ON claude_accounts
BEGIN
    DELETE FROM claude_connectors WHERE account_ref = OLD.id;
END;
