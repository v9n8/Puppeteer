// Unit tests for lib/projectSwitcher.ts (titlebar project switcher, UX-09).
// Run: node --test src/lib/__tests__/projectSwitcher.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  MRU_LIMIT, touchMru, previousProject, switcherRows, stepIndex, tabsToClose, wasRestored,
} from '../projectSwitcher.ts'

const p = (id, name, path = `/code/${id}`, hidden = false) => ({ id, name, path, hidden })
const ids = (rows) => rows.map((r) => r.project.id)

test('touchMru: moves the project to the front, once, capped', () => {
  assert.deepEqual(touchMru([], 'a'), ['a'])
  assert.deepEqual(touchMru(['a', 'b', 'c'], 'c'), ['c', 'a', 'b'])
  assert.deepEqual(touchMru(['a', 'b'], 'a'), ['a', 'b'], 'already first: unchanged')
  const long = Array.from({ length: MRU_LIMIT }, (_, i) => `p${i}`)
  const next = touchMru(long, 'new')
  assert.equal(next.length, MRU_LIMIT)
  assert.equal(next[0], 'new')
  assert.ok(!next.includes(`p${MRU_LIMIT - 1}`), 'the oldest falls off')
  assert.deepEqual(touchMru(['a'], ''), ['a'], 'no id: no change')
  const before = ['a', 'b']
  touchMru(before, 'b')
  assert.deepEqual(before, ['a', 'b'], 'the input is not mutated')
})

test('previousProject: the most recent other project that still exists', () => {
  const live = [{ id: 'a' }, { id: 'b' }, { id: 'c' }]
  assert.equal(previousProject(['a', 'b', 'c'], 'a', live), 'b')
  assert.equal(previousProject(['b', 'a'], 'a', live), 'b', 'current not first in the MRU')
  assert.equal(previousProject(['a', 'gone', 'c'], 'a', live), 'c', 'skips a removed project')
  assert.equal(previousProject(['a', 'b'], null, live), 'a', 'off a project screen: the last one used')
  assert.equal(previousProject(['a'], 'a', live), null, 'nothing to go back to')
  assert.equal(previousProject([], null, live), null)
})

test('switcherRows: recent first in MRU order, then the rest in store order', () => {
  const projects = [p('a', 'Alpha'), p('b', 'Beta'), p('c', 'Gamma'), p('d', 'Delta')]
  const rows = switcherRows(projects, { query: '', mru: ['c', 'a'], showHidden: false })
  assert.deepEqual(ids(rows), ['c', 'a', 'b', 'd'])
  assert.deepEqual(rows.map((r) => r.recent), [true, true, false, false])
})

test('switcherRows: case-insensitive substring of the name or the path', () => {
  const projects = [p('a', 'Web App', '/work/frontend'), p('b', 'API', '/work/backend'), p('c', 'docs', '/notes/site')]
  assert.deepEqual(ids(switcherRows(projects, { query: 'app', mru: [], showHidden: false })), ['a'])
  assert.deepEqual(ids(switcherRows(projects, { query: '  WORK ', mru: ['b'], showHidden: false })), ['b', 'a'],
    'path match, trimmed, recent still first')
  assert.deepEqual(ids(switcherRows(projects, { query: 'zzz', mru: [], showHidden: false })), [])
})

test('switcherRows: hidden projects only with showHidden', () => {
  const projects = [p('a', 'Shown'), p('b', 'Secret', '/x/b', true)]
  assert.deepEqual(ids(switcherRows(projects, { query: '', mru: ['b'], showHidden: false })), ['a'],
    'a hidden project stays out even when recently used')
  assert.deepEqual(ids(switcherRows(projects, { query: '', mru: ['b'], showHidden: true })), ['b', 'a'])
})

test('switcherRows: duplicate names (any case) show the path', () => {
  const projects = [p('a', 'api', '/one/api'), p('b', 'API', '/two/api'), p('c', 'web')]
  const rows = switcherRows(projects, { query: '', mru: [], showHidden: false })
  assert.deepEqual(rows.map((r) => r.showPath), [true, true, false])
  const filtered = switcherRows(projects, { query: '/one', mru: [], showHidden: false })
  assert.deepEqual(filtered.map((r) => r.showPath), [false], 'only names that collide among the shown rows')
})

test('stepIndex: arrow keys wrap at both ends', () => {
  assert.equal(stepIndex(0, 1, 3), 1)
  assert.equal(stepIndex(2, 1, 3), 0)
  assert.equal(stepIndex(0, -1, 3), 2)
  assert.equal(stepIndex(-1, 1, 3), 0)
  assert.equal(stepIndex(-1, -1, 3), 2)
  assert.equal(stepIndex(0, 1, 0), -1)
})

test('tabsToClose: other tabs, tabs to the right, all tabs', () => {
  const order = ['a', 'b', 'c', 'd']
  assert.deepEqual(tabsToClose(order, 'b', 'others'), ['a', 'c', 'd'])
  assert.deepEqual(tabsToClose(order, 'b', 'right'), ['c', 'd'])
  assert.deepEqual(tabsToClose(order, 'd', 'right'), [], 'nothing right of the last tab')
  assert.deepEqual(tabsToClose(['a'], 'a', 'others'), [], 'nothing else open')
  assert.deepEqual(tabsToClose(order, 'b', 'all'), order)
  assert.notEqual(tabsToClose(order, 'b', 'all'), order, 'a copy: closing mutates the store\'s list')
  assert.deepEqual(tabsToClose(order, 'x', 'others'), [], 'unknown tab closes nothing')
  assert.deepEqual(tabsToClose(order, 'x', 'right'), [])
})

test('wasRestored: old row or existing sessions mean it came back from the Trash', () => {
  const now = Date.parse('2026-11-10T10:00:00Z')
  assert.equal(wasRestored({ created_at: '2026-11-10T10:00:01+00:00', run_count: 0 }, now), false, 'new row')
  assert.equal(wasRestored({ created_at: '2026-11-10T09:59:30+00:00', run_count: 0 }, now), false, 'within the skew slack')
  assert.equal(wasRestored({ created_at: '2025-01-01T00:00:00+00:00', run_count: 0 }, now), true, 'old row')
  assert.equal(wasRestored({ created_at: '2026-11-10T10:00:01+00:00', run_count: 3 }, now), true, 'has sessions')
  assert.equal(wasRestored({ created_at: 'not a date', run_count: 0 }, now), false)
})
