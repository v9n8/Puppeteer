// Unit tests for lib/mcpBlocked.ts (the session's "MCP server blocked" notice).
// Run: node --test src/lib/__tests__/mcpBlocked.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { addMcpBlocked, addMcpBlockedAll, readMcpBlocked } from '../mcpBlocked.ts'

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

test('a listener that attached late adds the start\'s skipped servers once', () => {
  const backend = [
    { server: 'claude_ai_Slack-global', source: 'puppeteer', reason: 'skipped' },
    { server: 'pms', source: 'puppeteer', reason: 'skipped' },
  ]
  let list = addMcpBlocked([], { server: 'claude_ai_Slack', source: 'claudeai', reason: 'loaded' })
  list = addMcpBlockedAll(list, backend)
  // The same list read again (the event also arrived live) adds nothing.
  list = addMcpBlockedAll(list, backend)
  assert.deepEqual(list, [
    { server: 'claude_ai_Slack', source: 'claudeai', calls: 0 },
    { server: 'claude_ai_Slack-global', source: 'puppeteer', calls: 0, skipped: true },
    { server: 'pms', source: 'puppeteer', calls: 0, skipped: true },
  ])
  for (const raw of [null, undefined, {}, 'x', [null, 7]]) assert.deepEqual(addMcpBlockedAll([], raw), [])
})

test('the store reads the skipped servers back when it starts listening', () => {
  const src = readFileSync(new URL('../../stores/runMcpBlocked.ts', import.meta.url), 'utf8')
  const fn = (name) => src.slice(src.indexOf(`function ${name}(`), src.indexOf('\n  }\n', src.indexOf(`function ${name}(`)))
  assert.match(fn('readSkipped'), /invoke<unknown>\('list_run_mcp_skipped', \{ runId \}\)/)
  assert.match(fn('readSkipped'), /addMcpBlockedAll\(/)
  assert.match(fn('listenTo'), /readSkipped\(runId\)/)
})

test('a session shown without a live listener reads its skipped servers once', () => {
  const store = readFileSync(new URL('../../stores/runMcpBlocked.ts', import.meta.url), 'utf8')
  const once = store.slice(store.indexOf('function readSkippedOnce('), store.indexOf('\n  }\n', store.indexOf('function readSkippedOnce(')))
  assert.match(once, /if \(!skippedRead\.has\(runId\)\) readSkipped\(runId\)/)
  const notice = readFileSync(new URL('../../components/sessions/RunMcpBlockedNotice.vue', import.meta.url), 'utf8')
  assert.match(notice, /store\.readSkippedOnce\(id\)/)
  assert.match(notice, /\{ immediate: true \}/)
})

test('a call denied because the CLI gave no provenance is counted and flagged', () => {
  assert.equal(readMcpBlocked({ server: 'pms', reason: 'no_provenance' })?.reason, 'no_provenance')
  let list = addMcpBlocked([], { server: 'pms', source: 'unknown', reason: 'call' })
  list = addMcpBlocked(list, { server: 'pms', source: 'unknown', reason: 'no_provenance' })
  list = addMcpBlocked(list, { server: 'pms', source: 'unknown', reason: 'no_provenance' })
  list = addMcpBlocked(list, { server: 'docs', source: 'unknown', reason: 'no_provenance' })
  assert.deepEqual(list, [
    { server: 'pms', source: 'unknown', calls: 3, noProvenance: true },
    { server: 'docs', source: 'unknown', calls: 1, noProvenance: true },
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
