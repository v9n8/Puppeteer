// Unit tests for lib/projectHealth.ts (V3-48).
// Run: node --test src/lib/__tests__/projectHealth.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { clockText, healthRows, mcpToTest } from '../projectHealth.ts'

const ready = (value) => ({ state: 'ready', value })
const base = {
  repos: ready([]),
  engine: 'claude',
  account: ready(null),
  mcp: ready({ enabled: 0, checking: false, failures: [] }),
  conflicts: [],
  servers: ready([]),
}
const keys = (rows) => rows.map((r) => `${r.key}:${r.tone}`)

test('mcpToTest keeps servers on app-wide and for the project', () => {
  const list = [
    { name: 'a', enabled: true, enabled_for_project: true },
    { name: 'b', enabled: false, enabled_for_project: true },
    { name: 'c', enabled: true, enabled_for_project: false },
  ]
  assert.deepEqual(mcpToTest(list).map((s) => s.name), ['a'])
})

test('clockText pads hours and minutes', () => {
  assert.equal(clockText(new Date(2026, 10, 16, 9, 4).getTime()), '09:04')
})

test('a new project: no repo and no account are warnings with links, nothing else shows', () => {
  const rows = healthRows(base)
  assert.deepEqual(keys(rows), ['repos:warn', 'account:warn'])
  assert.deepEqual(rows[0].link, { target: 'repos', labelKey: 'projectDetail.health.linkRepos' })
  assert.equal(rows[1].title.key, 'projectDetail.health.claudeNoAccount')
  assert.equal(rows[1].link.target, 'session')
})

test('everything fine: repos with names, account, MCP count, servers', () => {
  const rows = healthRows({
    ...base,
    repos: ready(['a/x', 'a/y']),
    account: ready('Work'),
    mcp: ready({ enabled: 3, checking: false, failures: [] }),
    servers: ready(['Prod', 'Staging']),
  })
  assert.deepEqual(keys(rows), ['repos:ok', 'account:ok', 'mcp:ok', 'servers:ok'])
  assert.deepEqual(rows[0].subs, [{ text: 'a/x · a/y' }])
  assert.equal(rows[0].title.params.count, 2)
  assert.equal(rows[2].title.params.count, 3)
  assert.deepEqual(rows[3].subs, [{ text: 'Prod · Staging' }])
})

test('the codex engine words its account row for Codex', () => {
  const rows = healthRows({ ...base, engine: 'codex', account: ready('Main') })
  assert.equal(rows[1].title.key, 'projectDetail.health.codexAccount')
  assert.equal(rows[1].title.params.label, 'Main')
})

test('a failing MCP server gets its own row with the real message and the check time', () => {
  const at = new Date(2026, 10, 16, 9, 42).getTime()
  const rows = healthRows({
    ...base,
    mcp: ready({ enabled: 2, checking: false, failures: [{ name: 'sentry', message: 'timed out after 10 s', at }] }),
  })
  const row = rows.find((r) => r.key === 'mcp-sentry')
  assert.equal(row.tone, 'warn')
  assert.deepEqual(row.title.params, { name: 'sentry' })
  assert.deepEqual(row.subs[0].params, { time: '09:42', message: 'timed out after 10 s' })
  assert.equal(row.link.target, 'mcp')
  assert.ok(!rows.some((r) => r.key === 'mcp'), 'no "N on" row while one fails')
})

test('MCP still being tested shows a busy row, and failures found so far', () => {
  const loading = healthRows({ ...base, mcp: { state: 'loading' } })
  assert.deepEqual(keys(loading).filter((k) => k.startsWith('mcp')), ['mcp-busy:busy'])
  const partial = healthRows({
    ...base,
    mcp: ready({ enabled: 2, checking: true, failures: [{ name: 'x', message: 'm', at: 0 }] }),
  })
  assert.deepEqual(keys(partial).filter((k) => k.startsWith('mcp')), ['mcp-x:warn', 'mcp-busy:busy'])
})

test('no MCP server on: no MCP row', () => {
  assert.ok(!healthRows(base).some((r) => r.key.startsWith('mcp')))
})

test('conflicts: one row with a line per item, a link and the re-check action', () => {
  const one = healthRows({ ...base, conflicts: [{ id: '1', name: 'ui-review', kind: 'skill' }] })
  const row = one.find((r) => r.key === 'conflicts')
  assert.equal(row.title.params.count, 1)
  assert.equal(row.action, 'recheck')
  assert.equal(row.link.target, 'conflicts')
  assert.deepEqual(row.subs[0].params, { name: 'ui-review', kind: 'Skill' })

  const two = healthRows({
    ...base,
    conflicts: [{ id: '1', name: 'a', kind: 'skill' }, { id: '2', name: 'b', kind: 'rule' }],
  }).find((r) => r.key === 'conflicts')
  assert.equal(two.title.params.count, 2)
  assert.deepEqual(two.subs.map((s) => s.params.kind), ['Skill', 'Rule'])
})

test('a source that failed to load shows its error; the other rows stay', () => {
  const rows = healthRows({
    ...base,
    repos: ready(['a/x']),
    servers: { state: 'error', error: 'db locked' },
  })
  const bad = rows.find((r) => r.key === 'servers')
  assert.equal(bad.tone, 'warn')
  assert.equal(bad.title.key, 'projectDetail.health.serversLoadError')
  assert.deepEqual(bad.subs, [{ text: 'db locked' }])
  assert.ok(rows.some((r) => r.key === 'repos' && r.tone === 'ok'))
})

test('sources still loading add no row', () => {
  const rows = healthRows({ ...base, repos: { state: 'loading' }, account: { state: 'loading' } })
  assert.deepEqual(keys(rows), [])
})
