// MCP lockdown for Codex: a run loads and calls only the MCP servers Puppeteer
// passed (PUPPETEER_CODEX_MCP, forwarded as `-c mcp_servers.<name>.…`). Without
// it codex also starts every server of `$CODEX_HOME/config.toml` and of a
// trusted project's `.codex/config.toml`, plus ChatGPT apps and plugin servers.
//
// - Before start: every server `codex mcp list --json` names (run in the
//   session's cwd, so the project layer counts) that Puppeteer did not forward
//   gets `-c mcp_servers.<name>.enabled=false`, plus a placeholder of its own
//   transport (DISABLED_TRANSPORT); the features that can bring in a server of
//   their own (LOCKDOWN_FEATURES) are turned off. Session flags beat user and
//   project config, not managed (MDM) layers. The list failing → the run does
//   not start.
// - While running: an `mcpToolCall` item of a server not forwarded, or a server
//   codex reports starting, is reported; a call also interrupts the turn (codex
//   has no pre-call deny hook for MCP tools, so the call may already be under way).
//
// Each block is reported as `{ server, source: 'codex', reason }` (`call` |
// `loaded`), the Claude sidecar's `_puppeteer_mcp_blocked` shape.

import { spawnSync } from 'node:child_process'

// The MCP servers codex's own config defines for `cwd`, as `{ name, type }`
// (`type`: the transport codex reports, `stdio` | `streamable_http`, or null).
// Fail-closed: throws when the list cannot be read, so no session starts with
// them enabled.
export function configuredMcpServers(codexBin, cwd, env = process.env) {
  const r = spawnSync(codexBin, ['mcp', 'list', '--json'], {
    cwd, env, encoding: 'utf8', timeout: 10_000, stdio: ['ignore', 'pipe', 'ignore'],
  })
  if (r.error) throw new Error(`codex mcp list failed: ${r.error.message}`)
  if (r.status !== 0) throw new Error(`codex mcp list exited with ${r.status ?? r.signal}`)
  let list
  try {
    list = JSON.parse(r.stdout || '')
  } catch {
    throw new Error('codex mcp list printed no JSON')
  }
  if (!Array.isArray(list)) throw new Error('codex mcp list did not return a list')
  return list
    .filter((s) => typeof s?.name === 'string' && s.name)
    .map((s) => ({ name: s.name, type: typeof s.transport?.type === 'string' ? s.transport.type : null }))
}

// A value of each transport's launch field for a disabled server. codex 0.172.0
// reloads its config without the project layer on some calls (e.g.
// `mcpServerStatus/list`), where a project server's `enabled=false` flag stands
// alone and fails with "invalid transport". The placeholder keeps that table
// valid. It must match the server's own transport: a `command` on an http
// server makes the whole config invalid. It is never run or fetched: the
// server is disabled, and both would fail anyway.
const DISABLED_TRANSPORT = {
  stdio: 'command="puppeteer-mcp-disabled"',
  streamable_http: 'url="http://127.0.0.1:9/puppeteer-mcp-disabled"',
}

// `-c` overrides turning off each of `servers` (`configuredMcpServers`) not in
// `allowed`. Key paths split on dots and take no quoted segment (codex 0.172.0
// fails on `mcp_servers."a.b".enabled`), yet codex does load a quoted-key server
// such as `[mcp_servers."my.srv"]`. Such a name cannot be turned off, so it
// throws: fail closed, the session does not start. A transport codex did not
// name gets no placeholder: `enabled=false` alone still turns it off.
export function disableServerArgs(servers, allowed = new Set()) {
  const args = []
  for (const { name, type } of servers) {
    if (allowed.has(name)) continue
    if (!/^[A-Za-z0-9_-]+$/.test(name)) {
      throw new Error(`codex config has MCP server ${JSON.stringify(name)} whose name Puppeteer cannot turn off; rename it to letters, digits, _ or -`)
    }
    args.push('-c', `mcp_servers.${name}.enabled=false`)
    if (Object.hasOwn(DISABLED_TRANSPORT, type)) args.push('-c', `mcp_servers.${name}.${DISABLED_TRANSPORT[type]}`)
  }
  return args
}

// The servers a config's args forward: those given a command or a url.
export function forwardedServerNames(args) {
  const names = new Set()
  for (const a of args) {
    const m = /^mcp_servers\.([A-Za-z0-9_-]+)\.(command|url)=/.exec(String(a))
    if (m) names.add(m[1])
  }
  return names
}

// ChatGPT apps, local and remote plugins, the installer of a skill's MCP
// dependencies and tool suggestions (which install apps / plugins): each can
// add a server that `codex mcp list` does not name at start.
const LOCKDOWN_FEATURES = ['apps', 'plugins', 'remote_plugin', 'skill_mcp_dependency_install', 'tool_suggest']

// `config` (`buildMcpConfig()`'s `{ args, env }`) locked down for a normal run,
// with `allowed` = the forwarded server names. Throws when codex's list fails.
export function withMcpLockdown(config, codexBin, cwd) {
  const allowed = forwardedServerNames(config.args)
  const configured = configuredMcpServers(codexBin, cwd, config.env)
  return {
    ...config,
    args: [
      ...config.args,
      ...disableServerArgs(configured, allowed),
      ...LOCKDOWN_FEATURES.flatMap((f) => ['-c', `features.${f}=false`]),
    ],
    allowed,
  }
}

// Watches codex notifications for servers not in `allowed`. `report(block)`
// sends one `_puppeteer_mcp_blocked`; `interrupt()` stops the turn in flight.
// Returns `(method, params)`, called for every notification.
export function mcpCallGuard(allowed, { report, interrupt }) {
  const seenLoaded = new Set()
  return (method, params) => {
    if (method === 'mcpServer/startupStatus/updated') {
      const name = typeof params?.name === 'string' ? params.name : ''
      if (!name || allowed.has(name) || seenLoaded.has(name)) return
      seenLoaded.add(name)
      report({ server: name, source: 'codex', reason: 'loaded' })
      return
    }
    // Codex reports a tool call as it starts (`item/started`, before its result).
    if (method !== 'item/started' || params?.item?.type !== 'mcpToolCall') return
    const server = typeof params.item.server === 'string' ? params.item.server : ''
    if (allowed.has(server)) return
    report({ server: server || 'unknown', source: 'codex', reason: 'call' })
    interrupt()
  }
}
