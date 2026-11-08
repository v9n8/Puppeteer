#!/usr/bin/env node
// JSON-RPC fixture for test-framing-seed.mjs and test-rewind.mjs: records
// thread/start, thread/resume, thread/rollback and turn/start params to
// PUPPETEER_FRAMING_RPC_OUT (NDJSON).
import { appendFileSync } from 'node:fs'

const out = process.env.PUPPETEER_FRAMING_RPC_OUT
let buffer = ''
const send = (m) => process.stdout.write(JSON.stringify(m) + '\n')

process.stdin.on('data', (chunk) => {
  buffer += chunk.toString()
  let nl
  while ((nl = buffer.indexOf('\n')) >= 0) {
    const line = buffer.slice(0, nl).trim()
    buffer = buffer.slice(nl + 1)
    if (!line) continue
    let m
    try { m = JSON.parse(line) } catch { continue }
    if (['thread/start', 'thread/resume', 'thread/rollback', 'turn/start'].includes(m.method)) {
      appendFileSync(out, JSON.stringify({ method: m.method, params: m.params }) + '\n')
    }
    if (m.id === undefined) continue
    if (m.method === 'thread/start') {
      send({ jsonrpc: '2.0', id: m.id, result: { thread: { id: 'fixture-thread' }, model: 'fixture' } })
    } else if (m.method === 'thread/resume') {
      // PUPPETEER_FIXTURE_TURNS: how many turns the resumed thread holds.
      const held = process.env.PUPPETEER_FIXTURE_TURNS
      const turns = held ? { turns: Array.from({ length: Number(held) }, (_, i) => ({ id: `t${i}` })) } : {}
      send({ jsonrpc: '2.0', id: m.id, result: { thread: { id: m.params.threadId, ...turns }, model: 'fixture' } })
    } else if (m.method === 'turn/start') {
      send({ jsonrpc: '2.0', id: m.id, result: { turn: { id: `turn-${m.id}` } } })
    } else {
      send({ jsonrpc: '2.0', id: m.id, result: {} })
    }
  }
})
