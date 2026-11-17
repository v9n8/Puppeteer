// Unit tests for lib/flagFilter.ts and lib/settingsTargets.ts (V3-19).
// Run: node --test src/lib/__tests__/flagFilter.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { clearGoneFlags, filterByFlag, filterOptions, flagCounts, resetIfGone } from '../flagFilter.ts'
import { hasProjectSection, hasSettingsSection } from '../settingsTargets.ts'

const runs = [
  { id: 'a', flag_id: 'x' },
  { id: 'b', flag_id: 'y' },
  { id: 'c', flag_id: null },
  { id: 'd' },
  { id: 'w', flag_id: 'x', conductor_run_id: 'a' },
]

test('filterByFlag: all, none, one flag', () => {
  assert.equal(filterByFlag(runs, 'all').length, 5)
  assert.deepEqual(filterByFlag(runs, 'none').map((r) => r.id), ['c', 'd'])
  assert.deepEqual(filterByFlag(runs, 'x').map((r) => r.id), ['a', 'w'])
})

test('flagCounts leaves workers out', () => {
  const c = flagCounts(runs)
  assert.equal(c.total, 4)
  assert.equal(c.none, 2)
  assert.equal(c.byFlag.get('x'), 1)
  assert.equal(c.byFlag.get('y'), 1)
})

test('filterOptions: groups, counts; a hidden flag only while sessions carry it', () => {
  const flags = [
    { id: 'x', project_id: null, hidden: false },
    { id: 'y', project_id: null, hidden: true },
    { id: 'z', project_id: null, hidden: true },
    { id: 'p', project_id: 'proj', hidden: false },
  ]
  const o = filterOptions(flags, flagCounts(runs))
  assert.deepEqual(o.shared.map((s) => [s.flag.id, s.count]), [['x', 1], ['y', 1]])
  assert.deepEqual(o.project.map((s) => [s.flag.id, s.count]), [['p', 0]])
})

test('resetIfGone keeps all / none and a listed flag, resets a missing one', () => {
  const flags = [{ id: 'x' }]
  assert.equal(resetIfGone('all', flags), 'all')
  assert.equal(resetIfGone('none', []), 'none')
  assert.equal(resetIfGone('x', flags), 'x')
  assert.equal(resetIfGone('gone', flags), 'all')
})

test('settingsTargets: known ids only', () => {
  assert.equal(hasSettingsSection('general'), true)
  assert.equal(hasSettingsSection('nope'), false)
  assert.equal(hasProjectSection('overview'), true)
  assert.equal(hasProjectSection('nope'), false)
})

test('clearGoneFlags drops a flag id that no longer exists, only for projects whose list is loaded (F-R3-9)', () => {
  const rows = [
    { id: 'a', project_id: 'p1', flag_id: 'gone' },
    { id: 'b', project_id: 'p1', flag_id: 'kept' },
    { id: 'c', project_id: 'p2', flag_id: 'gone' },
    { id: 'd', project_id: 'p1', flag_id: null },
  ]
  const flagsOf = (projectId) => (projectId === 'p1' ? [{ id: 'kept' }] : null)
  assert.equal(clearGoneFlags(rows, flagsOf), 1)
  assert.deepEqual(rows.map((r) => r.flag_id), [null, 'kept', 'gone', null])
})
