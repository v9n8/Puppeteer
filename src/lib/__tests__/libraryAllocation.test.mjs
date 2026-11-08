// Unit tests for lib/libraryAllocation.ts (Rules / Agents / Commands allocation matrix).
// Run: node --test src/lib/__tests__/libraryAllocation.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { buildItemMatrix, itemChipSwitchable } from '../libraryAllocation.ts'

const rules = {
  engines: ['claude', 'codex'],
  projects: [
    { id: 'p2', name: 'two' },
    { id: 'p1', name: 'one' },
  ],
  items: [
    { id: 'a', name: 'style', description: 'Tabs or spaces', engines: ['claude', 'codex'] },
    { id: 'b', name: 'ci', description: 'Pin actions', engines: ['codex'] },
  ],
  chips: [
    { item_id: 'a', project_id: 'p2', engine: 'claude', enabled: true, state: 'installed', held: false },
    { item_id: 'a', project_id: 'p2', engine: 'codex', enabled: true, state: 'drifted', held: false },
    { item_id: 'b', project_id: 'p1', engine: 'codex', enabled: true, state: 'installed', held: true },
    { item_id: 'b', project_id: 'p2', engine: 'claude', enabled: false, state: 'git_managed', held: false },
  ],
}

const brief = (row) => row.cells.map((c) => [c.projectId, c.applied, c.held, c.chips.map((x) => `${x.engine}:${x.state}:${x.enabled}`)])

test('buildItemMatrix: a cell per project, a chip per engine, off-target engines marked', () => {
  const rows = buildItemMatrix(rules, '')
  assert.deepEqual(rows.map((r) => r.item.id), ['a', 'b'])
  assert.deepEqual(brief(rows[0]), [
    ['p2', true, false, ['claude:installed:true', 'codex:drifted:true']],
    ['p1', false, false, ['claude:not_installed:false', 'codex:not_installed:false']],
  ])
  // `ci` targets Codex: its Claude chip is off target unless the disk says otherwise.
  assert.deepEqual(brief(rows[1]), [
    ['p2', false, false, ['claude:git_managed:false', 'codex:not_installed:false']],
    ['p1', true, true, ['claude:off_target:false', 'codex:installed:true']],
  ])
})

test('buildItemMatrix: search reaches name and description', () => {
  assert.deepEqual(buildItemMatrix(rules, 'pin').map((r) => r.item.id), ['b'])
  assert.deepEqual(buildItemMatrix(rules, 'STYLE').map((r) => r.item.id), ['a'])
  assert.deepEqual(buildItemMatrix(rules, 'nothing'), [])
})

test('buildItemMatrix: one Claude column for agents and commands', () => {
  const agents = {
    engines: ['claude'],
    projects: [{ id: 'p1', name: 'one' }],
    items: [{ id: 'r', name: 'reviewer', description: 'd', engines: ['claude'] }],
    chips: [],
  }
  assert.deepEqual(brief(buildItemMatrix(agents, '')[0]), [['p1', false, false, ['claude:not_installed:false']]])
})

test('itemChipSwitchable: not the repo file, not off target, not while a group keeps it', () => {
  const [style, ci] = buildItemMatrix(rules, '')
  assert.equal(itemChipSwitchable(style.cells[0], style.cells[0].chips[1]), true, 'a drifted chip switches off')
  assert.equal(itemChipSwitchable(style.cells[1], style.cells[1].chips[0]), true)
  assert.equal(itemChipSwitchable(ci.cells[0], ci.cells[0].chips[0]), false, 'git-managed')
  assert.equal(itemChipSwitchable(ci.cells[0], ci.cells[0].chips[1]), true)
  assert.equal(itemChipSwitchable(ci.cells[1], ci.cells[1].chips[0]), false, 'off target')
  assert.equal(itemChipSwitchable(ci.cells[1], ci.cells[1].chips[1]), false, 'held by a group')
})
