#!/usr/bin/env node
// MCP probe: which MCP servers the Claude CLI reports for one profile — the ones no
// file shows: claude.ai connectors (S1), plugin servers (S5), managed servers (S6).
// No model call: the prompt stream never yields; the SDK starts the CLI, the probe
// waits for the servers to settle and asks `mcpServerStatus()`.
//
// The probe reads no user / project / local settings (`settingSources: []`), so the
// CLI does not start the profile's user-scope servers of `.claude.json` (S2/S3/S4 come
// from reading the files, `mcp_discovery::claude_sources`). Measured with SDK 0.3.319
// on the global profile: the claude.ai connectors are listed all the same (26 of 26),
// user-scope servers started 2 → 0. The plugins the profile's own `settings.json`
// enables are passed back as flag settings (`enabledPlugins` only), so their servers
// still load and are listed. What still runs: those plugins' MCP servers and the
// managed policy tier (always read) start for the settle time. Not listed any more:
// servers of plugins synced from claude.ai or enabled only in a project's settings
// (runs block them all the same). No `projects` entry was added to `.claude.json` by
// a probe (checked with fresh cwds, keys only).
//
// Run by the app (`mcp_discovery::probe`) with the run's env: CLAUDE_CONFIG_DIR of
// the profile (none for the global one), PUPPETEER_CLAUDE_PATH when set.
//   node mcp-probe.mjs [cwd]
//
// Prints one JSON line per server, then a `done` line; on failure one `error` line
// and exit 1. A line holds only name / source / status / transport, and for a
// claude.ai connector `{ id, url }` (what a run passes back to allow it). Never a
// command, an argument, an env or a header.
//
//   {"type":"server","name":"claude.ai Slack","source":"claudeai","status":"connected","transport":"claudeai-proxy","connector":{"id":"…","url":"…"}}
//   {"type":"done","count":1}
//   {"type":"error","error":"…"}
//
// Env (tests): PUPPETEER_MCP_PROBE_SETTLE_MS (default 12000) how long servers get to
// connect; PUPPETEER_MCP_PROBE_TIMEOUT_MS (default 45000) the hard limit.

import { query } from '@anthropic-ai/claude-agent-sdk'
import { readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

const SETTLE_MS = Number(process.env.PUPPETEER_MCP_PROBE_SETTLE_MS ?? 12_000)
const TIMEOUT_MS = Number(process.env.PUPPETEER_MCP_PROBE_TIMEOUT_MS ?? 45_000)
const TEXT_CAP = 200

// Write the lines, then exit once they are out: on macOS a pipe is written
// asynchronously, so an exit right after `write` could cut the output.
function finish(lines, code) {
  process.stdout.write(lines.map((l) => JSON.stringify(l) + '\n').join(''), () => process.exit(code))
}

// Server names and statuses come from config files and the network: one line, capped.
function text(value) {
  return typeof value === 'string' ? value.replace(/[\u0000-\u001f\u007f]/g, ' ').slice(0, TEXT_CAP) : null
}

// A prompt stream that never yields: the CLI starts, no turn is ever sent.
function never() {
  return { [Symbol.asyncIterator]() { return { next: () => new Promise(() => {}) } } }
}

// Only what discovery needs from one `mcpServerStatus()` entry.
export function probeLine(entry) {
  const config = entry?.config ?? {}
  const out = {
    type: 'server',
    name: text(entry?.name),
    source: text(entry?.source) ?? 'unknown',
    status: text(entry?.status) ?? 'unknown',
    transport: text(config.type),
  }
  if (config.type === 'claudeai-proxy' && typeof config.id === 'string' && typeof config.url === 'string') {
    out.connector = { id: config.id, url: config.url }
  }
  return out
}

// The `enabledPlugins` of the profile's user settings (the CLI's own path rule), or
// null when the file or the key cannot be read. Nothing else of the file goes on.
export function enabledPluginsOf(env) {
  try {
    const dir = env.CLAUDE_CONFIG_DIR || join(homedir(), '.claude')
    const plugins = JSON.parse(readFileSync(join(dir, 'settings.json'), 'utf8'))?.enabledPlugins
    return plugins && typeof plugins === 'object' && !Array.isArray(plugins) ? plugins : null
  } catch {
    return null
  }
}

// The SDK options of a probe in `cwd` (see the header for why no settings source).
export function probeOptions(cwd, env) {
  const options = { cwd, persistSession: false, permissionMode: 'default', settingSources: [] }
  const enabledPlugins = enabledPluginsOf(env)
  if (enabledPlugins) options.settings = { enabledPlugins }
  if (env.PUPPETEER_CLAUDE_PATH) options.pathToClaudeCodeExecutable = env.PUPPETEER_CLAUDE_PATH
  return options
}

async function main() {
  const abortController = new AbortController()
  const options = { ...probeOptions(process.argv[2] || process.cwd(), process.env), abortController }
  const timeout = setTimeout(() => {
    abortController.abort()
    finish([{ type: 'error', error: `no answer within ${TIMEOUT_MS} ms` }], 1)
  }, TIMEOUT_MS)
  try {
    const q = query({ prompt: never(), options })
    // Drain in the background so control responses flow.
    ;(async () => { try { for await (const _ of q) { /* nothing to read */ } } catch { /* ended by the abort */ } })()
    await new Promise((resolve) => setTimeout(resolve, SETTLE_MS))
    const status = await q.mcpServerStatus()
    if (!Array.isArray(status)) throw new Error('mcpServerStatus() did not return a list')
    const lines = status.map(probeLine).filter((l) => l.name)
    clearTimeout(timeout)
    abortController.abort()
    finish([...lines, { type: 'done', count: lines.length }], 0)
  } catch (err) {
    clearTimeout(timeout)
    abortController.abort()
    finish([{ type: 'error', error: text(String(err?.message ?? err).split('\n')[0]) || 'probe failed' }], 1)
  }
}

// Run as a script; imported by the test for `probeLine`.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await main()
