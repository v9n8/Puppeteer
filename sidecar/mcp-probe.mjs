#!/usr/bin/env node
// MCP probe: which MCP servers the Claude CLI loads for one profile, as a normal
// unlocked run would — claude.ai connectors, plugin and managed servers included.
// No model call: the prompt stream never yields; the SDK starts the CLI, the probe
// waits for the servers to settle and asks `mcpServerStatus()`.
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

async function main() {
  const abortController = new AbortController()
  const options = { cwd: process.argv[2] || process.cwd(), persistSession: false, permissionMode: 'default', abortController }
  if (process.env.PUPPETEER_CLAUDE_PATH) options.pathToClaudeCodeExecutable = process.env.PUPPETEER_CLAUDE_PATH
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
