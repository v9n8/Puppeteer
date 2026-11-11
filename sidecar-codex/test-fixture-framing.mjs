#!/usr/bin/env node
// JSON-RPC fixture for test-framing-seed.mjs, test-rewind.mjs, test-fork.mjs,
// test-steer.mjs, test-queue.mjs and test-sandbox-policy.mjs: records
// thread/start, thread/resume, thread/fork, thread/rollback, thread/revert,
// turn/start and turn/steer params to PUPPETEER_FRAMING_RPC_OUT (NDJSON). A
// turn never completes, so it stays steerable — unless
// PUPPETEER_FIXTURE_COMPLETE_MS is set: then it completes after that long
// (recorded as `turn/completed`).
// It answers like codex-cli 0.133 (thread/rollback, no thread/revert) — or,
// with PUPPETEER_FIXTURE_CODEX=head, like openai/codex@d15bf1d: no
// thread/rollback, thread/revert, and threads in the paginated history format
// (PUPPETEER_FIXTURE_HISTORY_MODE names another, `none` leaves it out).
import { appendFileSync } from 'node:fs'

const out = process.env.PUPPETEER_FRAMING_RPC_OUT
let buffer = ''
let refusedTurn = false
const send = (m) => process.stdout.write(JSON.stringify(m) + '\n')
const head = process.env.PUPPETEER_FIXTURE_CODEX === 'head'
const historyMode = process.env.PUPPETEER_FIXTURE_HISTORY_MODE || 'paginated'
const known = head ? (method) => method !== 'thread/rollback' : (method) => method !== 'thread/revert'

process.stdin.on('data', (chunk) => {
  buffer += chunk.toString()
  let nl
  while ((nl = buffer.indexOf('\n')) >= 0) {
    const line = buffer.slice(0, nl).trim()
    buffer = buffer.slice(nl + 1)
    if (!line) continue
    let m
    try { m = JSON.parse(line) } catch { continue }
    if (['thread/start', 'thread/resume', 'thread/fork', 'thread/rollback', 'thread/revert', 'turn/start', 'turn/steer'].includes(m.method)) {
      appendFileSync(out, JSON.stringify({ id: m.id, method: m.method, params: m.params }) + '\n')
    }
    if (m.id === undefined) continue
    if (!known(m.method)) {
      // How the app-server refuses a method it does not have.
      send({ jsonrpc: '2.0', id: m.id, error: { code: -32600, message: `Invalid request: unknown variant \`${m.method}\`, expected one of \`initialize\`` } })
    } else if (m.method === 'thread/start') {
      send({ jsonrpc: '2.0', id: m.id, result: { thread: { id: 'fixture-thread' }, model: 'fixture' } })
    } else if (m.method === 'thread/resume' || m.method === 'thread/fork') {
      // PUPPETEER_FIXTURE_REFUSE_POLICY: refuse a policy on resume/fork, like a
      // codex whose app-server does not know the fields.
      if (process.env.PUPPETEER_FIXTURE_REFUSE_POLICY && ('approvalPolicy' in m.params || 'sandbox' in m.params)) {
        send({ jsonrpc: '2.0', id: m.id, error: { code: -32600, message: 'Invalid request: unknown field `approvalPolicy`' } })
        continue
      }
      // PUPPETEER_FIXTURE_TURNS: how many turns the resumed (or copied) thread holds.
      const held = process.env.PUPPETEER_FIXTURE_TURNS
      const turns = held ? { turns: Array.from({ length: Number(held) }, (_, i) => ({ id: `t${i}` })) } : {}
      const mode = head && historyMode !== 'none' ? { historyMode } : {}
      // A fork is a new thread — unless PUPPETEER_FIXTURE_FORK_SAME_ID (a broken server).
      const id = m.method === 'thread/fork' && !process.env.PUPPETEER_FIXTURE_FORK_SAME_ID ? `fork-of-${m.params.threadId}` : m.params.threadId
      send({ jsonrpc: '2.0', id: m.id, result: { thread: { id, ...mode, ...turns }, model: 'fixture' } })
    } else if (m.method === 'turn/start') {
      // PUPPETEER_FIXTURE_FAIL_FIRST_TURN: refuse the first turn/start.
      if (process.env.PUPPETEER_FIXTURE_FAIL_FIRST_TURN && !refusedTurn) {
        refusedTurn = true
        send({ jsonrpc: '2.0', id: m.id, error: { code: -32600, message: 'bad input' } })
        continue
      }
      const turn = { id: `turn-${m.id}` }
      send({ jsonrpc: '2.0', id: m.id, result: { turn } })
      const completeMs = process.env.PUPPETEER_FIXTURE_COMPLETE_MS
      if (completeMs) {
        setTimeout(() => {
          appendFileSync(out, JSON.stringify({ method: 'turn/completed', params: { turnId: turn.id } }) + '\n')
          send({ jsonrpc: '2.0', method: 'turn/completed', params: { threadId: m.params.threadId, turn } })
        }, Number(completeMs))
      }
    } else if (m.method === 'turn/steer') {
      // PUPPETEER_FIXTURE_STEER_ERROR: refuse it like a turn that just ended.
      const refuse = process.env.PUPPETEER_FIXTURE_STEER_ERROR
      send(refuse
        ? { jsonrpc: '2.0', id: m.id, error: { code: -32600, message: refuse } }
        : { jsonrpc: '2.0', id: m.id, result: { turnId: m.params.expectedTurnId } })
    } else {
      send({ jsonrpc: '2.0', id: m.id, result: {} })
    }
  }
})
