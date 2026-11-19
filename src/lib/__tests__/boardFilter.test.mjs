// Unit tests for lib/boardFilter.ts (UR-60): the board's column filter.
// Run: node --test src/lib/__tests__/boardFilter.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  defaultFilter, filterColumns, hiddenCount, isDefaultFilter, listedFlags, parseFilter, reconcileFilter, sameFilter,
  reorderFlags, reorderGroups, serializeFilter, toggleColumn, ALL_SCOPE, SHARED_GROUP,
} from '../boardFilter.ts'

const flag = (id, extra = {}) => ({
  id, project_id: null, name: id, color: 'info', position: 0, builtin_key: null, stock_name: false, hidden: false, run_count: 0,
  ...extra,
})
// All projects: two shared flags, then own flags of p2 and p1 (the backend lists them by Session activity).
const all = [flag('a'), flag('b'), flag('q', { project_id: 'p2' }), flag('p', { project_id: 'p1' }), flag('p-2', { project_id: 'p1' })]
// One Project: a shared flag it hides, and its own.
const one = [flag('a'), flag('b', { hidden: true }), flag('p', { project_id: 'p1' })]
const order = ['p1', 'p2']
const ids = (cols) => cols.map((c) => c.id)

test('listedFlags: one Project leaves out the shared flags it hides; All lists every flag', () => {
  assert.deepEqual(listedFlags(one, 'p1').map((f) => f.id), ['a', 'p'])
  assert.deepEqual(listedFlags(all, ALL_SCOPE).map((f) => f.id), ['a', 'b', 'q', 'p', 'p-2'])
})

test('defaultFilter: shared flags on, No flag and own flags off; groups shared then the Project list order', () => {
  const f = defaultFilter(all, ALL_SCOPE, order)
  assert.deepEqual(f.on, ['a', 'b'])
  assert.deepEqual(f.groups, [SHARED_GROUP, 'p1', 'p2'])
  assert.deepEqual(f.flags, { shared: ['a', 'b'], p1: ['p', 'p-2'], p2: ['q'] })
  assert.deepEqual(ids(filterColumns(f, all, ALL_SCOPE)), ['a', 'b'])
  // A Project not in the list (yet) still gets its group, after the listed ones.
  assert.deepEqual(defaultFilter(all, ALL_SCOPE, ['p1']).groups, [SHARED_GROUP, 'p1', 'p2'])
  // One Project: only its own group, and its hidden shared flag is not offered.
  assert.deepEqual(defaultFilter(one, 'p1'), { on: ['a'], groups: [SHARED_GROUP, 'p1'], flags: { shared: ['a'], p1: ['p'] } })
})

test('filterColumns: No flag first when on, then groups and flags in the filter order, only those on', () => {
  const f = {
    on: ['none', 'b', 'q', 'p-2', 'p'],
    groups: ['p2', SHARED_GROUP, 'p1'],
    flags: { shared: ['b', 'a'], p1: ['p-2', 'p'], p2: ['q'] },
  }
  assert.deepEqual(ids(filterColumns(f, all, ALL_SCOPE)), ['none', 'q', 'b', 'p-2', 'p'])
  // A flag id that is not on the board any more is skipped.
  assert.deepEqual(ids(filterColumns({ ...f, flags: { ...f.flags, p2: ['q', 'gone'] }, on: [...f.on, 'gone'] }, all, ALL_SCOPE)),
    ['none', 'q', 'b', 'p-2', 'p'])
  // One Project: the hidden shared flag never shows, even when the saved filter has it on.
  assert.deepEqual(ids(filterColumns({ on: ['a', 'b', 'p'], groups: [SHARED_GROUP, 'p1'], flags: { shared: ['a', 'b'], p1: ['p'] } }, one, 'p1')),
    ['a', 'p'])
})

test('reorder: the columns follow the group and flag order the filter holds', () => {
  const f = { ...defaultFilter(all, ALL_SCOPE, order), on: ['a', 'b', 'p', 'p-2', 'q'] }
  assert.deepEqual(ids(filterColumns(f, all, ALL_SCOPE)), ['a', 'b', 'p', 'p-2', 'q'])
  const moved = { ...f, groups: [SHARED_GROUP, 'p2', 'p1'], flags: { ...f.flags, p1: ['p-2', 'p'] } }
  assert.deepEqual(ids(filterColumns(moved, all, ALL_SCOPE)), ['a', 'b', 'q', 'p-2', 'p'])
})

test('reorderGroups / reorderFlags (UR-62): the board columns follow the dropped order', () => {
  const f = { ...defaultFilter(all, ALL_SCOPE, order), on: ['none', 'a', 'b', 'p', 'p-2', 'q'] }
  const g = reorderGroups(f, ['p2', SHARED_GROUP, 'p1'])
  assert.deepEqual(g.groups, ['p2', SHARED_GROUP, 'p1'])
  assert.deepEqual(ids(filterColumns(g, all, ALL_SCOPE)), ['none', 'q', 'a', 'b', 'p', 'p-2'])
  // A group the popover does not show stays, at the end.
  assert.deepEqual(reorderGroups({ ...f, groups: [...f.groups, 'empty'] }, ['p1', SHARED_GROUP, 'p2']).groups,
    ['p1', SHARED_GROUP, 'p2', 'empty'])
  const fl = reorderFlags(g, 'p1', ['p-2', 'p'])
  assert.deepEqual(ids(filterColumns(fl, all, ALL_SCOPE)), ['none', 'q', 'a', 'b', 'p-2', 'p'])
  // A flag of another group cannot be moved in.
  assert.deepEqual(reorderFlags(f, 'p1', ['q', 'p-2', 'p']).flags.p1, ['p-2', 'p'])
  assert.deepEqual(reorderFlags(f, 'p1', ['q', 'p-2', 'p']).flags.p2, ['q'])
  // The input is left as it was (the popover's undo needs the old draft).
  assert.deepEqual(f.groups, [SHARED_GROUP, 'p1', 'p2'])
  assert.equal(sameFilter(f, fl), false)
})

test('reconcileFilter: nothing saved is the default', () => {
  assert.deepEqual(reconcileFilter(null, all, ALL_SCOPE, order), defaultFilter(all, ALL_SCOPE, order))
})

test('reconcileFilter: new flags join the end of their group (shared on, own off); gone ones drop; new Projects join last', () => {
  const saved = {
    on: ['none', 'b', 'gone', 'p'],
    groups: ['p1', SHARED_GROUP, 'p-gone'],
    flags: { shared: ['b', 'gone'], p1: ['p'], 'p-gone': ['x'] },
  }
  const withNew = [...all, flag('c'), flag('p-3', { project_id: 'p1' })]
  const f = reconcileFilter(saved, withNew, ALL_SCOPE, order)
  assert.deepEqual(f.groups, ['p1', SHARED_GROUP, 'p2'])
  assert.deepEqual(f.flags, { p1: ['p', 'p-2', 'p-3'], shared: ['b', 'a', 'c'], p2: ['q'] })
  // 'a' was known (unticked) only if saved listed it: here it was not, so as a new shared flag it is on.
  assert.deepEqual([...f.on].sort(), ['a', 'b', 'c', 'none', 'p'])
  // A flag the user unticked stays off.
  const kept = reconcileFilter({ ...saved, flags: { ...saved.flags, shared: ['b', 'a'] } }, all, ALL_SCOPE, order)
  assert.ok(!kept.on.includes('a'))
})

test('isDefaultFilter: same columns as the default, whatever the order of what is off', () => {
  const base = defaultFilter(all, ALL_SCOPE, order)
  assert.equal(isDefaultFilter(base, all, ALL_SCOPE, order), true)
  assert.equal(isDefaultFilter({ ...base, groups: [SHARED_GROUP, 'p2', 'p1'] }, all, ALL_SCOPE, order), true)
  assert.equal(isDefaultFilter(toggleColumn(base, 'none'), all, ALL_SCOPE, order), false)
  assert.equal(isDefaultFilter({ ...base, flags: { ...base.flags, shared: ['b', 'a'] } }, all, ALL_SCOPE, order), false)
})

test('sameFilter and toggleColumn', () => {
  const base = defaultFilter(all, ALL_SCOPE, order)
  assert.equal(sameFilter(base, { ...base, on: [...base.on].reverse() }), true)
  assert.equal(sameFilter(base, toggleColumn(base, 'p')), false)
  assert.equal(sameFilter(base, toggleColumn(toggleColumn(base, 'p'), 'p')), true)
  assert.equal(sameFilter(base, { ...base, groups: [SHARED_GROUP, 'p2', 'p1'] }), false)
  assert.equal(sameFilter(base, { ...base, flags: { ...base.flags, p1: ['p-2', 'p'] } }), false)
})

test('hiddenCount: cards whose column is off (No flag included)', () => {
  const cards = [{ flag_id: null }, { flag_id: 'a' }, { flag_id: 'p' }, { flag_id: 'q' }]
  const cols = filterColumns(defaultFilter(all, ALL_SCOPE, order), all, ALL_SCOPE)
  assert.equal(hiddenCount(cards, cols), 3)
  assert.equal(hiddenCount(cards, []), 4)
})

test('parseFilter / serializeFilter: round trip per scope; broken input gives the default', () => {
  const saved = { all: defaultFilter(all, ALL_SCOPE, order), p1: defaultFilter(one, 'p1') }
  assert.deepEqual(parseFilter(serializeFilter(saved)), saved)
  assert.deepEqual(parseFilter(null), {})
  assert.deepEqual(parseFilter('{nope'), {})
  assert.deepEqual(parseFilter('[1]'), {})
  // A broken scope is left out; the good one stays.
  assert.deepEqual(parseFilter(JSON.stringify({ all: { on: 'x' }, p1: saved.p1 })), { p1: saved.p1 })
  assert.deepEqual(parseFilter(JSON.stringify({ p1: { on: [], groups: [], flags: { a: [1] } } })), {})
})
