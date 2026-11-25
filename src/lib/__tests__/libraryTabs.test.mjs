// Unit tests for lib/libraryTabs.ts (V3-49).
// Run: node --test src/lib/__tests__/libraryTabs.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { DISK_ITEM_KINDS } from '../libraryDisk.ts'
import { kindCounts, libraryKinds, pickTab, projectListCollapsed, tabRows } from '../libraryTabs.ts'

/** A project's file kinds and Memory, as the Library lists them in remote mode. */
const PROJECT_ITEM_KINDS = libraryKinds('project', true)

const item = (kind, name, engine = 'claude', description = '') => ({
  kind, name, engine, description, path: `/h/${engine}/${kind}/${name}`, status: 'file',
})

const items = [
  item('skill', 'pr-review', 'codex'),
  item('skill', 'pr-review', 'claude', 'Review a PR'),
  item('skill', 'deploy'),
  item('rule', 'deploy-checklist'),
  item('rule', 'style'),
  item('agent', 'tester', 'claude', 'Writes tests'),
  item('memory', 'CLAUDE.md'),
]

test('kindCounts counts every kind, zero included, and ignores kinds not asked for', () => {
  assert.deepEqual(kindCounts(items, PROJECT_ITEM_KINDS), { skill: 3, rule: 2, command: 0, agent: 1, memory: 1 })
  assert.deepEqual(kindCounts(items, DISK_ITEM_KINDS), { skill: 3, rule: 2, command: 0, agent: 1 })
})

test('tabRows keeps one kind, sorted by name then engine', () => {
  const rows = tabRows(items, 'skill', '')
  assert.deepEqual(rows.map((r) => r.item.name), ['deploy', 'pr-review', 'pr-review'])
  assert.deepEqual(rows.map((r) => r.item.engine).slice(1), ['claude', 'codex'])
})

test('tabRows search only filters inside the tab', () => {
  assert.deepEqual(tabRows(items, 'rule', 'deploy').map((r) => r.item.name), ['deploy-checklist'])
  assert.deepEqual(tabRows(items, 'agent', 'deploy'), [])
  assert.deepEqual(tabRows(items, 'agent', 'writes').map((r) => r.item.name), ['tester'])
})

test('pickTab follows the selected file, even an agent', () => {
  assert.equal(pickTab(items, PROJECT_ITEM_KINDS, '/h/claude/agent/tester', 'skill'), 'agent')
})

test('pickTab keeps the current tab while it has items', () => {
  assert.equal(pickTab(items, PROJECT_ITEM_KINDS, null, 'rule'), 'rule')
})

test('pickTab leaves a tab that has no items any more', () => {
  assert.equal(pickTab(items, PROJECT_ITEM_KINDS, null, 'command'), 'skill')
  assert.equal(pickTab([item('agent', 'a')], PROJECT_ITEM_KINDS, null, 'skill'), 'agent')
})

test('pickTab on an empty place is the first kind', () => {
  assert.equal(pickTab([], PROJECT_ITEM_KINDS, null, null), 'skill')
})

test('pickTab on Global never lands on Memory', () => {
  assert.equal(pickTab(items, DISK_ITEM_KINDS, '/h/claude/memory/CLAUDE.md', 'memory'), 'skill')
  assert.equal(pickTab([item('memory', 'CLAUDE.md')], DISK_ITEM_KINDS, null, null), 'skill')
})

test('pickTab ignores Memory rows whose file is not on disk', () => {
  const absent = [{ ...item('memory', 'CLAUDE.md'), hash: null }, { ...item('memory', 'AGENTS.md', 'codex'), hash: null }]
  assert.equal(pickTab(absent, PROJECT_ITEM_KINDS, null, null), 'skill')
  assert.equal(pickTab([{ ...absent[0], hash: 'h' }], PROJECT_ITEM_KINDS, null, null), 'memory')
})

test('projectListCollapsed: wide uses the saved choice, narrow folds unless opened', () => {
  assert.equal(projectListCollapsed(false, false, null), false)
  assert.equal(projectListCollapsed(true, false, null), true)
  assert.equal(projectListCollapsed(true, false, false), true, 'override ignored when wide')
  assert.equal(projectListCollapsed(false, true, null), true, 'folds by itself when narrow')
  assert.equal(projectListCollapsed(false, true, false), false, 'opened by hand holds while narrow')
  assert.equal(projectListCollapsed(true, true, false), false, 'saved fold does not apply when narrow')
})
