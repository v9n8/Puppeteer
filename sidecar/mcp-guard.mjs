// MCP lockdown: a Claude run loads and calls only the MCP servers Puppeteer
// passed in `options.mcpServers`. Without it the CLI also loads servers Puppeteer
// does not manage: the account's claude.ai connectors, `~/.claude.json` user and
// local scope, a project's `.mcp.json`, plugin servers and agent frontmatter.
//
// Two walls:
// - `strictMcpConfig` makes the CLI ignore every MCP config but the one passed;
// - a `PreToolUse` hook (and the permission callback, when there is one) denies
//   an MCP tool unless its server is one Puppeteer passed. The CLI tells where
//   a tool's server came from (`mcp_server.source`); a passed server is
//   `dynamic` (or `sdk`, in-process). Anything else, or no source at all (an
//   older CLI), is denied: trust is keyed on the source, never on the name.
//   The built-in resource tools name their server in `tool_input.server`
//   instead: a read of a server not passed, or of one the init listed from
//   another source, is denied; a list of every server's resources too, once
//   a server not passed is loaded. Hooks run in every permission mode,
//   `bypassPermissions` included.
//
// Each block is reported to the app as
//   { "type": "_puppeteer_mcp_blocked", server, source, reason }
// reason `call` (a tool call was denied) or `loaded` (the CLI still started a
// server that was not passed — e.g. a managed policy one; its tools stay denied).
// `server` is untrusted text from the config that declared it.

const PASSED_SOURCES = new Set(['dynamic', 'sdk'])

export const MCP_BLOCKED_REASON =
  'This MCP server is not managed by Puppeteer, so its tools cannot be used in this session. Add it in Puppeteer\'s MCP settings to use it.'

// The server part of `mcp__<server>__<tool>`, for the report only.
function serverOfTool(toolName) {
  const rest = String(toolName).slice('mcp__'.length)
  const end = rest.indexOf('__')
  return end > 0 ? rest.slice(0, end) : rest
}

// Built-in tools that list or read a connected server's resources.
const RESOURCE_TOOLS = ['ListMcpResourcesTool', 'ReadMcpResourceTool', 'ReadMcpResourceDirTool']

// Like `mcpCallVerdict`, for the resource tools: `null` when the call may go
// ahead (not a resource tool, or a passed server's resources), `{ server,
// source }` to report, or `{ server: null }` (a list of every server while one
// not passed is loaded: deny, nothing new to report). `untrusted` is the names
// the init listed from a source other than a passed one.
export function resourceCallVerdict(toolName, toolInput, allowed, untrusted) {
  if (!RESOURCE_TOOLS.includes(toolName)) return null
  const server = toolInput?.server
  if (server === undefined && toolName === 'ListMcpResourcesTool') return untrusted.size ? { server: null } : null
  if (typeof server === 'string' && allowed.has(server) && !untrusted.has(server)) return null
  return { server: typeof server === 'string' ? server : 'unknown', source: 'unknown' }
}

// `null` when the call may go ahead (not an MCP tool, or a passed server's);
// else `{ server, source }` to report. `provenance` is the CLI's
// `{ name, source }` for the tool's server; `allowed` the passed server names.
export function mcpCallVerdict(toolName, provenance, allowed) {
  const isMcp = provenance != null || String(toolName ?? '').startsWith('mcp__')
  if (!isMcp) return null
  if (typeof provenance?.name !== 'string' || typeof provenance?.source !== 'string') {
    return { server: serverOfTool(toolName ?? ''), source: 'unknown' }
  }
  if (PASSED_SOURCES.has(provenance.source) && allowed.has(provenance.name)) return null
  return { server: provenance.name, source: provenance.source }
}

// Locks `options` down and returns `onMessage(message)`, which reports the
// servers a `system/init` lists beyond the passed ones. `send` writes one
// control message to the broker.
export function applyMcpLockdown(options, send) {
  options.strictMcpConfig = true
  const allowed = new Set(Object.keys(options.mcpServers ?? {}))
  const report = ({ server, source }, reason) => {
    try { send({ type: '_puppeteer_mcp_blocked', server, source, reason }) } catch { /* the deny still holds */ }
  }

  // Filled from the init (below), before any tool call.
  const untrusted = new Set()
  // The call's verdict; a server to report is reported here.
  const verdictOf = (toolName, toolInput, provenance) => {
    const verdict = resourceCallVerdict(toolName, toolInput, allowed, untrusted) ?? mcpCallVerdict(toolName, provenance, allowed)
    if (verdict?.server != null) report(verdict, 'call')
    return verdict
  }

  const guard = async (input) => {
    if (!verdictOf(input?.tool_name, input?.tool_input, input?.mcp_server)) return {}
    return { hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: 'deny', permissionDecisionReason: MCP_BLOCKED_REASON } }
  }
  const matcher = ['mcp__.*', ...RESOURCE_TOOLS].join('|')
  options.hooks = { ...options.hooks, PreToolUse: [...(options.hooks?.PreToolUse ?? []), { matcher, hooks: [guard] }] }

  const ask = options.canUseTool
  if (typeof ask === 'function') {
    options.canUseTool = async (toolName, input, opts) => {
      if (!verdictOf(toolName, input, opts?.mcpServer)) return ask(toolName, input, opts)
      return { behavior: 'deny', message: MCP_BLOCKED_REASON }
    }
  }

  // A server is reported once per session, however many inits list it.
  const seen = new Set()
  return (message) => {
    if (message?.type !== 'system' || message.subtype !== 'init' || !Array.isArray(message.mcp_servers)) return
    for (const entry of message.mcp_servers) {
      const name = typeof entry?.name === 'string' ? entry.name : ''
      // `source` is absent on CLIs that predate it: the name alone decides then.
      const source = typeof entry?.source === 'string' ? entry.source : undefined
      if (allowed.has(name) && (source === undefined || PASSED_SOURCES.has(source))) continue
      untrusted.add(name)
      const key = `${name}\u0000${source ?? ''}`
      if (seen.has(key)) continue
      seen.add(key)
      report({ server: name, source: source ?? 'unknown' }, 'loaded')
    }
  }
}
