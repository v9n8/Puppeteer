// Unit tests for lib/sidebarNav.ts (sidebar groups, Session target, start page).
// Run: node --test src/lib/__tests__/sidebarNav.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { homePath, navActive, sessionNavTarget, sessionPath, sidebarGroups } from '../sidebarNav.ts'

const prop = (id, effective) => ({ id, effective })
const tree = (...props) => [{ module: 'm', enabled: true, props }]
const allOn = tree(prop('productivity.focus', true), prop('productivity.todos', true), prop('productivity.notes', true))

const items = [
  { id: '/stats', group: 'foot' },
  { id: '/projects', group: 'work' },
  { id: '/search', group: 'work', sidebar: false },
  { id: '/session', group: 'work' },
  { id: 'notes', group: 'mine', feature: 'productivity.notes' },
  { id: '/tasks', group: 'mine', feature: 'productivity.focus|productivity.todos' },
  { id: '/library', group: 'agent' },
  { id: '/trash', group: 'foot' },
  { id: '/settings', group: 'foot' },
]

test('sidebarGroups: four groups in order, entries in table order', () => {
  const groups = sidebarGroups(items, allOn)
  assert.deepEqual(groups.map((g) => g.id), ['work', 'mine', 'agent', 'foot'])
  assert.deepEqual(groups[0].items.map((i) => i.id), ['/session', '/projects'])
  assert.deepEqual(groups[1].items.map((i) => i.id), ['/tasks', 'notes'])
  assert.deepEqual(groups[3].items.map((i) => i.id), ['/stats', '/trash', '/settings'])
  assert.equal(groups[0].labelKey, 'nav.groups.work')
  assert.equal(groups[3].labelKey, null)
})

test('sidebarGroups: /search stays out of the sidebar', () => {
  const ids = sidebarGroups(items, allOn).flatMap((g) => g.items.map((i) => i.id))
  assert.ok(!ids.includes('/search'))
})

test('sidebarGroups: a group with every entry off loses its heading', () => {
  const groups = sidebarGroups(items, tree())
  assert.deepEqual(groups.map((g) => g.id), ['work', 'agent', 'foot'])
  // Tree not loaded yet behaves the same: only ungated entries.
  assert.deepEqual(sidebarGroups(items, []).map((g) => g.id), ['work', 'agent', 'foot'])
})

test('navActive: Session and Project do not both light up', () => {
  assert.equal(navActive('/session', '/projects/p1'), true)
  assert.equal(navActive('/session', '/projects/p1/run/r1'), true)
  assert.equal(navActive('/session', '/projects'), false)
  assert.equal(navActive('/projects', '/projects'), true)
  assert.equal(navActive('/projects', '/projects/p1/run/r1'), false)
  assert.equal(navActive('/library', '/library'), true)
  assert.equal(navActive('/settings', '/settings/general'), true)
  assert.equal(navActive('/stats', '/statsx'), false)
})

const tabs = [
  { projectId: 'a', lastRunId: 'ra' },
  { projectId: 'b', lastRunId: null },
]

test('sessionNavTarget: the tab being viewed beats the most recent one', () => {
  assert.deepEqual(sessionNavTarget(tabs, ['b', 'a'], 'a'), { projectId: 'a', runId: 'ra' })
  assert.deepEqual(sessionNavTarget(tabs, ['b', 'a'], null), { projectId: 'b', runId: null })
})

test('sessionNavTarget: skips recent projects with no tab, then falls back to the first tab', () => {
  assert.deepEqual(sessionNavTarget(tabs, ['gone', 'a'], 'elsewhere'), { projectId: 'a', runId: 'ra' })
  assert.deepEqual(sessionNavTarget(tabs, [], null), { projectId: 'a', runId: 'ra' })
})

test('sessionNavTarget: no tabs gives null', () => {
  assert.equal(sessionNavTarget([], ['a'], 'a'), null)
})

test('homePath: last used tab, else the Projects screen', () => {
  assert.equal(homePath(tabs, ['b', 'a']), '/projects/b')
  assert.equal(homePath([], ['a']), '/projects')
})

test('sessionPath: the last run when the tab has one', () => {
  assert.equal(sessionPath({ projectId: 'a', runId: 'ra' }), '/projects/a/run/ra')
  assert.equal(sessionPath({ projectId: 'a', runId: null }), '/projects/a')
})
