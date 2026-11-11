#!/usr/bin/env node
// Deterministic JSON-RPC fixture for test-deny.mjs (and the other approval
// tests), and — with PUPPETEER_FIXTURE_USAGE — for test-usage.mjs and
// test-turn-outcome.mjs.
import { writeFileSync, appendFileSync } from 'node:fs'

// `codex mcp list --json` (asked by a tool-less sidecar): one configured server,
// or — PUPPETEER_FIXTURE_MCP_LIST=fail|garbage|object — a failing / unparsable answer.
if (process.argv.includes('mcp')) {
  const mode = process.env.PUPPETEER_FIXTURE_MCP_LIST
  if (mode === 'fail') {
    process.stderr.write('boom')
    process.exit(2)
  }
  if (mode === 'garbage') process.stdout.write('not json')
  else if (mode === 'object') process.stdout.write('{"servers":[]}')
  else process.stdout.write(JSON.stringify([{ name: 'user_server', enabled: true }]))
  process.exit(0)
}
// When set, record argv and every client request as JSON lines.
const fixtureLog = process.env.PUPPETEER_FIXTURE_LOG
if (fixtureLog) appendFileSync(fixtureLog, JSON.stringify({ argv: process.argv.slice(2) }) + '\n')

const marker = process.env.PUPPETEER_DENY_TEST_MARKER
const mcpMode = process.env.PUPPETEER_MCP_PERMISSION_KEY_TEST === '1'
// Codex's real app-server v2 elicitation has no `toolName` field; this mode
// reproduces that so the per-server permission key stays covered.
const mcpOmitToolName = process.env.PUPPETEER_MCP_OMIT_TOOLNAME === '1'
// When set, record the approval decision the sidecar sent so a test can assert
// it (e.g. `acceptForSession` for an "always" allow).
const decisionOut = process.env.PUPPETEER_DECISION_OUT
let buffer = ''

function send(message) {
  process.stdout.write(JSON.stringify(message) + '\n')
}

// Usage mode: PUPPETEER_FIXTURE_USAGE is a JSON script
//   { replay?: TokenUsageBreakdown, replayLate?: bool, turns: [{ fail?: bool, updates?: [ThreadTokenUsage], noTurnId?: bool,
//     errors?: [{ error: TurnError, willRetry: bool }], status?: TurnStatus, error?: TurnError }] }
// `replay` is the stored total codex sends on thread/resume (right after its
// reply, or with `replayLate` after the first turn/start reply); turn N plays
// turns[N]: refused, or turn/started + its usage updates + its `error`
// notifications + turn/completed (with `status`, default completed, and `error`).
const usageScript = process.env.PUPPETEER_FIXTURE_USAGE ? JSON.parse(process.env.PUPPETEER_FIXTURE_USAGE) : null
let usageTurn = 0
const replayUsage = () => send({
  jsonrpc: '2.0',
  method: 'thread/tokenUsage/updated',
  params: { threadId: 'fixture-thread', turnId: 'old-turn', tokenUsage: { total: usageScript.replay, last: usageScript.replay, modelContextWindow: null } },
})

function handleUsage(message) {
  if (message.id === undefined) return
  if (message.method === 'thread/start' || message.method === 'thread/resume') {
    send({ jsonrpc: '2.0', id: message.id, result: { thread: { id: 'fixture-thread' }, model: 'fixture' } })
    if (message.method === 'thread/resume' && usageScript.replay && !usageScript.replayLate) replayUsage()
  } else if (message.method === 'turn/start') {
    const n = usageTurn++
    const script = usageScript.turns[n] || {}
    if (script.fail) {
      send({ jsonrpc: '2.0', id: message.id, error: { code: -32600, message: 'bad input' } })
      return
    }
    const turn = { id: `turn-${n}` }
    send({ jsonrpc: '2.0', id: message.id, result: { turn } })
    if (n === 0 && usageScript.replay && usageScript.replayLate) replayUsage()
    // Later, like a real model request: never in the same read as the reply.
    setTimeout(() => {
      send({ jsonrpc: '2.0', method: 'turn/started', params: { threadId: 'fixture-thread', turn } })
      for (const tokenUsage of script.updates || []) {
        const params = { threadId: 'fixture-thread', tokenUsage }
        if (!script.noTurnId) params.turnId = turn.id
        send({ jsonrpc: '2.0', method: 'thread/tokenUsage/updated', params })
      }
      for (const e of script.errors || []) {
        send({ jsonrpc: '2.0', method: 'error', params: { ...e, threadId: 'fixture-thread', turnId: turn.id } })
      }
      const done = { ...turn, status: script.status || 'completed', error: script.error ?? null }
      send({ jsonrpc: '2.0', method: 'turn/completed', params: { threadId: 'fixture-thread', turn: done } })
    }, 30)
  } else {
    send({ jsonrpc: '2.0', id: message.id, result: {} })
  }
}

process.stdin.on('data', (chunk) => {
  buffer += chunk.toString()
  let newline
  while ((newline = buffer.indexOf('\n')) >= 0) {
    const line = buffer.slice(0, newline).trim()
    buffer = buffer.slice(newline + 1)
    if (!line) continue
    let message
    try { message = JSON.parse(line) } catch { continue }
    if (fixtureLog && message.method) appendFileSync(fixtureLog, JSON.stringify({ method: message.method, params: message.params }) + '\n')
    if (usageScript) { handleUsage(message); continue }

    if (message.method === 'initialize') {
      send({ jsonrpc: '2.0', id: message.id, result: {} })
    } else if (message.method === 'thread/start') {
      send({
        jsonrpc: '2.0',
        id: message.id,
        result: { thread: { id: 'fixture-thread' }, model: 'fixture' },
      })
    } else if (message.method === 'account/rateLimits/read') {
      send({ jsonrpc: '2.0', id: message.id, result: {} })
    } else if (message.method === 'turn/start') {
      send({ jsonrpc: '2.0', id: message.id, result: { turn: { id: 'fixture-turn' } } })
      if (mcpMode) {
        send({
          jsonrpc: '2.0',
          id: 92,
          method: 'mcpServer/elicitation/request',
          params: {
            serverName: 'Google Drive',
            ...(mcpOmitToolName ? {} : { toolName: 'list_files' }),
            mode: 'permission',
            message: 'Allow listing Drive files?',
          },
        })
        continue
      }
      send({
        jsonrpc: '2.0',
        id: 91,
        method: 'item/commandExecution/requestApproval',
        params: { command: `touch ${marker}`, reason: 'fixture approval' },
      })
    } else if (message.id === 91 && message.result) {
      const decision = message.result.decision
      if (decisionOut) writeFileSync(decisionOut, String(decision))
      const denied = decision === 'cancel' || decision === 'decline'
      if (!denied && marker) writeFileSync(marker, 'executed')
      send({
        jsonrpc: '2.0',
        method: 'item/completed',
        params: {
          item: {
            id: 'fixture-command',
            type: 'commandExecution',
            command: `touch ${marker}`,
            status: denied ? 'declined' : 'completed',
            exitCode: denied ? null : 0,
            aggregatedOutput: '',
          },
        },
      })
      send({
        jsonrpc: '2.0',
        method: 'turn/completed',
        params: { turn: { id: 'fixture-turn', status: 'completed' } },
      })
    } else if (message.id === 92 && message.result) {
      send({
        jsonrpc: '2.0',
        method: 'turn/completed',
        params: { turn: { id: 'fixture-turn', status: 'completed' } },
      })
    }
  }
})
