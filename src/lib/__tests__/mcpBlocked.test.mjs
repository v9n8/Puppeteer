// Unit tests for lib/mcpBlocked.ts (the session's "MCP server blocked" notice).
// Run: node --test src/lib/__tests__/mcpBlocked.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { addMcpBlocked, readMcpBlocked } from '../mcpBlocked.ts'

test('reads a report, defaulting source and reason', () => {
  assert.deepEqual(readMcpBlocked({ server: 'claude_ai_Slack', source: 'claudeai', reason: 'loaded' }), {
    server: 'claude_ai_Slack',
    source: 'claudeai',
    reason: 'loaded',
  })
  assert.deepEqual(readMcpBlocked({ server: 'x' }), { server: 'x', source: 'unknown', reason: 'call' })
  assert.deepEqual(readMcpBlocked({ server: 'x', reason: 'other' })?.reason, 'call')
})

test('drops unusable reports', () => {
  for (const raw of [null, undefined, 'x', 7, {}, { server: '' }, { server: 3 }]) {
    assert.equal(readMcpBlocked(raw), null)
    assert.deepEqual(addMcpBlocked([], raw), [])
  }
})

test('keeps one entry per server and source, counting denied calls', () => {
  let list = []
  list = addMcpBlocked(list, { server: 'claude.ai Slack', source: 'claudeai', reason: 'loaded' })
  list = addMcpBlocked(list, { server: 'claude_ai_Slack', source: 'claudeai', reason: 'call' })
  list = addMcpBlocked(list, { server: 'claude_ai_Slack', source: 'claudeai', reason: 'call' })
  list = addMcpBlocked(list, { server: 'pms', source: 'user', reason: 'call' })
  list = addMcpBlocked(list, { server: 'pms', source: 'project', reason: 'call' })
  assert.deepEqual(list, [
    { server: 'claude.ai Slack', source: 'claudeai', calls: 0 },
    { server: 'claude_ai_Slack', source: 'claudeai', calls: 2 },
    { server: 'pms', source: 'user', calls: 1 },
    { server: 'pms', source: 'project', calls: 1 },
  ])
})

test('keeps a server left out at start apart from the blocked ones', () => {
  assert.equal(readMcpBlocked({ server: 'x', source: 'puppeteer', reason: 'skipped' })?.reason, 'skipped')
  let list = []
  list = addMcpBlocked(list, { server: 'claude_ai_Slack', source: 'claudeai', reason: 'loaded' })
  list = addMcpBlocked(list, { server: 'claude_ai_Slack-global', source: 'puppeteer', reason: 'skipped' })
  // A resume reports it again: still one entry.
  list = addMcpBlocked(list, { server: 'claude_ai_Slack-global', source: 'puppeteer', reason: 'skipped' })
  assert.deepEqual(list, [
    { server: 'claude_ai_Slack', source: 'claudeai', calls: 0 },
    { server: 'claude_ai_Slack-global', source: 'puppeteer', calls: 0, skipped: true },
  ])
})

test('never changes the list it was given', () => {
  const list = [{ server: 'a', source: 'user', calls: 1 }]
  const next = addMcpBlocked(list, { server: 'a', source: 'user' })
  assert.deepEqual(list, [{ server: 'a', source: 'user', calls: 1 }])
  assert.equal(next[0].calls, 2)
})

test('a session dropped by the live-runs store drops its notice too', () => {
  const src = readFileSync(new URL('../../stores/liveRuns.ts', import.meta.url), 'utf8')
  const discard = src.slice(src.indexOf('function discard('), src.indexOf('\n  }\n', src.indexOf('function discard(')))
  assert.match(discard, /useRunMcpBlockedStore\(\)\.dismiss\(runId\)/)
})
