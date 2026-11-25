// Unit tests for lib/mcpDiscovery.ts (MCP servers found outside Puppeteer, as the MCP views list them).
// Run: node --test src/lib/__tests__/mcpDiscovery.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  blockedCountOf,
  conflictTone,
  isBlockedRow,
  isConnectorServer,
  outcomeCounts,
  projectSections,
  rowAction,
  rowConflicts,
  splitRows,
  viewKey,
} from '../mcpDiscovery.ts'

function row(over = {}) {
  return {
    id: 'S2|/h/.claude.json|pms',
    source: 'S2',
    scope: 'global',
    originLabel: '~/.claude.json',
    profile: null,
    projectId: null,
    app: null,
    name: 'pms',
    importName: 'pms',
    transport: 'http',
    summary: null,
    engineStatus: null,
    importable: true,
    state: 'not_managed',
    serverId: null,
    problem: null,
    conflicts: [],
    loadedInRuns: false,
    ...over,
  }
}

const GLOBAL = { ref: 'global', engine: 'claude', label: null }
const WORK = { ref: 'acc-1', engine: 'claude', label: 'Work' }
const HOME = { ref: 'acc-2', engine: 'claude', label: 'Home' }

function connector(profile, name, over = {}) {
  return row({
    id: `S1|${profile.ref}|${name}`,
    source: 'S1',
    originLabel: 'claude.ai',
    profile,
    name,
    importName: name.replace('claude.ai ', 'claude_ai_'),
    transport: 'connector',
    ...over,
  })
}

test('each state offers its action', () => {
  assert.equal(rowAction(row()), 'import')
  assert.equal(rowAction(row({ state: 'managed', serverId: 'm1' })), 'open')
  assert.equal(rowAction(row({ state: 'changed', serverId: 'm1' })), 'sync')
  assert.equal(rowAction(row({ state: 'blocked', importable: false })), 'blocked')
  assert.equal(rowAction(row({ state: 'unreadable', problem: 'bad entry' })), 'problem')
})

test('a row without what its action needs falls back to blocked', () => {
  assert.equal(rowAction(row({ importable: false })), 'blocked')
  assert.equal(rowAction(row({ state: 'managed' })), 'blocked')
  assert.equal(rowAction(row({ state: 'changed' })), 'blocked')
})

test('blocked count follows the backend rule: not managed, blocked or unreadable, other apps left out', () => {
  const rows = [
    row(),
    row({ state: 'blocked' }),
    row({ state: 'unreadable' }),
    row({ state: 'managed', serverId: 'm1' }),
    row({ state: 'changed', serverId: 'm2' }),
    row({ source: 'S9', app: 'cursor' }),
  ]
  assert.deepEqual(rows.map(isBlockedRow), [true, true, true, false, false, false])
  assert.equal(blockedCountOf(rows), 3)
  assert.equal(blockedCountOf([]), 0)
})

test('connectors group per profile; servers keep their order', () => {
  const rows = [
    connector(GLOBAL, 'claude.ai Slack', { conflicts: ['C1'] }),
    connector(WORK, 'claude.ai Slack', { conflicts: ['C1', 'C3'] }),
    row(),
    connector(WORK, 'claude.ai Drive', { conflicts: ['C1'] }),
    row({ id: 'S7|/h/.codex/config.toml|x', source: 'S7', name: 'x' }),
  ]
  const profiles = [
    { ...GLOBAL, known: true, sameLoginAs: ['acc-1'], conflicts: ['C1'] },
    { ...WORK, known: true, sameLoginAs: ['global'], conflicts: ['C1'] },
    { ...HOME, known: false, sameLoginAs: [], conflicts: ['C4'] },
  ]
  const split = splitRows(rows, profiles, [])
  assert.deepEqual(split.servers.map((r) => r.name), ['pms', 'x'])
  assert.deepEqual(
    split.connectors.map((g) => [g.profile.ref, g.known, g.conflicts, g.rows.map((r) => r.name)]),
    [
      ['global', true, ['C1'], ['claude.ai Slack']],
      ['acc-1', true, ['C1'], ['claude.ai Slack', 'claude.ai Drive']],
      // A profile without connectors still shows (its C4 label).
      ['acc-2', false, ['C4'], []],
    ],
  )
  assert.deepEqual(split.connectors[1].profile, WORK)
})

test('a profile only the rows name gets a group with the rows\' login labels', () => {
  const split = splitRows([connector(HOME, 'claude.ai Slack', { conflicts: ['C2', 'C3'] })], [], [])
  assert.deepEqual(split.connectors.map((g) => [g.profile.ref, g.known, g.conflicts]), [['acc-2', true, ['C2']]])
})

test('a failed connector probe sits on its profile; other problems stay apart', () => {
  const probe = { source: 'S1', label: 'claude.ai', message: 'no login', profile: WORK, projectId: null, app: null }
  const file = { source: 'S2', label: '~/.claude.json', message: 'line 3', profile: null, projectId: null, app: null }
  const stray = { ...probe, profile: { ref: 'gone', engine: 'claude', label: null } }
  const split = splitRows([], [{ ...WORK, known: true, sameLoginAs: [], conflicts: [] }], [probe, file, stray])
  assert.deepEqual(split.connectors[0].problems, [probe])
  assert.deepEqual(split.problems, [file, stray])
})

test('a connector row shows C3 itself; its profile labels are on the group', () => {
  assert.deepEqual(rowConflicts(connector(WORK, 'claude.ai Slack', { conflicts: ['C1', 'C3'] })), ['C3'])
  assert.deepEqual(rowConflicts(connector(WORK, 'claude.ai Slack', { conflicts: ['C4'] })), [])
  assert.deepEqual(rowConflicts(row({ conflicts: ['C3'] })), ['C3'])
  assert.deepEqual(['C1', 'C2', 'C3', 'C4'].map(conflictTone), ['info', 'info', 'warning', 'warning'])
})

test('a project view groups Project then Global, each with its own problems', () => {
  const own = row({ id: 'S4|/p/.mcp.json|y', source: 'S4', scope: 'project', projectId: 'p1', name: 'y' })
  const global = row()
  const projectProblem = { source: 'S4', label: '/p/.mcp.json', message: 'bad', profile: null, projectId: 'p1', app: null }
  const globalProblem = { ...projectProblem, source: 'S2', projectId: null }
  const sections = projectSections({ project: [own], global: [global], problems: [projectProblem, globalProblem] })
  assert.deepEqual(
    sections.map((s) => [s.key, s.rows.map((r) => r.name), s.problems.map((p) => p.source)]),
    [
      ['project', ['y'], ['S4']],
      ['global', ['pms'], ['S2']],
    ],
  )
})

test('connector servers, view keys and import counts', () => {
  assert.equal(isConnectorServer({ transport: 'claudeai' }), true)
  assert.equal(isConnectorServer({ transport: 'stdio' }), false)
  assert.equal(viewKey(null), '')
  assert.equal(viewKey(undefined), '')
  assert.equal(viewKey('p1'), 'p1')
  assert.deepEqual(
    outcomeCounts([{ status: 'imported' }, { status: 'imported' }, { status: 'skipped' }, { status: 'failed' }]),
    { imported: 2, skipped: 1, failed: 1 },
  )
})
