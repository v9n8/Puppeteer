// MCP lockdown for Codex: a run loads and calls only the MCP servers Puppeteer
// passed (PUPPETEER_CODEX_MCP, forwarded as `-c mcp_servers.<name>.…`). Without
// it codex also starts every server of `$CODEX_HOME/config.toml` and of a
// trusted project's `.codex/config.toml`, plus ChatGPT apps and plugin servers.
//
// - Before start: every server `codex mcp list --json` names (run in the
//   session's cwd, so the project layer counts) that Puppeteer did not forward
//   gets `-c mcp_servers.<name>.enabled=false`; `features.apps` and
//   `features.plugins` are turned off. Session flags beat user and project
//   config, not managed (MDM) layers. The list failing → the run does not start.
// - While running: an `mcpToolCall` item of a server not forwarded, or a server
//   codex reports starting, is reported; a call also interrupts the turn (codex
//   has no pre-call deny hook for MCP tools, so the call may already be under way).
//
// Each block is reported as `{ server, source: 'codex', reason }` (`call` |
// `loaded`), the Claude sidecar's `_puppeteer_mcp_blocked` shape.

import { spawnSync } from 'node:child_process'

// Names of the MCP servers codex's own config defines for `cwd`. Fail-closed:
// throws when the list cannot be read, so no session starts with them enabled.
export function configuredMcpServerNames(codexBin, cwd, env = process.env) {
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
  return list.map((s) => s?.name).filter((n) => typeof n === 'string' && n)
}

// `-c` overrides turning off each of `names` not in `allowed`. Key paths split
// on dots and take no quoted segment (codex 0.172.0 fails on
// `mcp_servers."a.b".enabled`), yet codex does load a quoted-key server such as
// `[mcp_servers."my.srv"]`. Such a name cannot be turned off, so it throws:
// fail closed, the session does not start.
export function disableServerArgs(names, allowed = new Set()) {
  const args = []
  for (const name of names) {
    if (allowed.has(name)) continue
    if (!/^[A-Za-z0-9_-]+$/.test(name)) {
      throw new Error(`codex config has MCP server ${JSON.stringify(name)} whose name Puppeteer cannot turn off; rename it to letters, digits, _ or -`)
    }
    args.push('-c', `mcp_servers.${name}.enabled=false`)
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

// `config` (`buildMcpConfig()`'s `{ args, env }`) locked down for a normal run,
// with `allowed` = the forwarded server names. Throws when codex's list fails.
export function withMcpLockdown(config, codexBin, cwd) {
  const allowed = forwardedServerNames(config.args)
  const configured = configuredMcpServerNames(codexBin, cwd, config.env)
  return {
    ...config,
    args: [
      ...config.args,
      ...disableServerArgs(configured, allowed),
      '-c', 'features.apps=false',
      '-c', 'features.plugins=false',
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
