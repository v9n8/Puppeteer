// Unit tests for lib/flagBoard.ts (V3-22).
// Run: node --test src/lib/__tests__/flagBoard.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  boardColumns, canDrop, cardStatus, cardTitle, dropFlagId, filterCards, groupCards, moveSelection, resolveBoardProject,
  NO_FLAG_COLUMN, OWN_FLAGS_COLUMN,
} from '../flagBoard.ts'

const flag = (id, extra = {}) => ({
  id, project_id: null, name: id, color: 'info', position: 0, builtin_key: null, stock_name: false, hidden: false, run_count: 0,
  ...extra,
})
const flags = [flag('a'), flag('b', { hidden: true }), flag('c'), flag('mine', { project_id: 'p1' })]
const ids = (cols) => cols.map((c) => c.id)

test('one project: No flag, the shared flags it does not hide, then its own', () => {
  assert.deepEqual(ids(boardColumns(flags, 'project')), [NO_FLAG_COLUMN, 'a', 'c', 'mine'])
})

test('all projects: every shared flag, then an Own flags column that takes no drop', () => {
  const cols = boardColumns([flag('a'), flag('b', { hidden: true }), flag('mine', { project_id: 'p1' })], 'all')
  assert.deepEqual(ids(cols), [NO_FLAG_COLUMN, 'a', 'b', OWN_FLAGS_COLUMN])
  assert.equal(cols.at(-1).droppable, false)
})

test('cards go to their flag column; an unknown flag falls back to No flag, or Own flags when all', () => {
  const cards = [
    { id: '1', flag_id: 'a' }, { id: '2', flag_id: null }, { id: '3', flag_id: 'b' }, { id: '4', flag_id: 'mine' },
  ]
  const one = groupCards(cards, boardColumns(flags, 'project'))
  assert.deepEqual(one.get('a').map((c) => c.id), ['1'])
  assert.deepEqual(one.get(NO_FLAG_COLUMN).map((c) => c.id), ['2', '3'])
  const all = groupCards(cards, boardColumns(flags, 'all'))
  assert.deepEqual(all.get(OWN_FLAGS_COLUMN).map((c) => c.id), ['4'])
  assert.deepEqual(all.get('b').map((c) => c.id), ['3'])
})

test('the filter keeps the name matches and counts matched / total', () => {
  const grouped = new Map([['a', [{ t: 'SSO login' }, { t: 'VNPay' }]], ['c', []]])
  const out = filterCards(grouped, ' sso ', (c) => c.t)
  assert.deepEqual(out.get('a'), { cards: [{ t: 'SSO login' }], matched: 1, total: 2 })
  assert.deepEqual(out.get('c'), { cards: [], matched: 0, total: 0 })
  assert.equal(filterCards(grouped, '', (c) => c.t).get('a').matched, 2)
})

test('canDrop: not on Own flags, not on a flag the card project hides, own flags only in their project', () => {
  const cols = boardColumns(flags, 'all')
  const col = (id) => cols.find((c) => c.id === id)
  const hidden = [{ project_id: 'p1', flag_id: 'b' }]
  assert.equal(canDrop({ project_id: 'p1' }, col(OWN_FLAGS_COLUMN), hidden), false)
  assert.equal(canDrop({ project_id: 'p1' }, col('b'), hidden), false)
  assert.equal(canDrop({ project_id: 'p2' }, col('b'), hidden), true)
  assert.equal(canDrop({ project_id: 'p1' }, col(NO_FLAG_COLUMN), hidden), true)
  const own = boardColumns(flags, 'project').find((c) => c.id === 'mine')
  assert.equal(canDrop({ project_id: 'p1' }, own, []), true)
  assert.equal(canDrop({ project_id: 'p2' }, own, []), false)
})

test('dropFlagId: the flag of a flag column, null for No flag', () => {
  const cols = boardColumns(flags, 'project')
  assert.equal(dropFlagId(cols[0]), null)
  assert.equal(dropFlagId(cols[1]), 'a')
})

const layout = {
  columnIds: ['none', 'a', 'c'],
  cardIds: { none: ['x', 'y'], a: [], c: ['z'] },
  dropColumnIds: ['none', 'a', 'c'],
}
const press = (state, ...keys) => keys.reduce((acc, k) => ({ ...moveSelection(layout, acc.state, k), prev: acc.state }), { state, effect: null })

test('keyboard: arrows move the focus, skipping empty columns sideways; Enter opens', () => {
  const start = { cursor: 'x', lifted: null }
  assert.equal(press(start, 'ArrowDown').state.cursor, 'y')
  assert.equal(press(start, 'ArrowUp').state.cursor, 'x')
  assert.equal(press(start, 'ArrowRight').state.cursor, 'z')
  assert.deepEqual(press(start, 'Enter').effect, { kind: 'open', runId: 'x' })
})

test('keyboard: Space lifts, arrows pick the column, Space drops, Esc puts it back', () => {
  const start = { cursor: 'x', lifted: null }
  const lifted = press(start, ' ', 'ArrowRight').state
  assert.deepEqual(lifted.lifted, { runId: 'x', from: 'none', target: 'a' })
  assert.deepEqual(moveSelection(layout, lifted, ' ').effect, { kind: 'drop', runId: 'x', columnId: 'a' })
  assert.deepEqual(moveSelection(layout, lifted, 'Escape'), { state: { cursor: 'x', lifted: null }, effect: { kind: 'cancel' } })
  // Dropping back where it was is a cancel, and the edge does not wrap.
  assert.deepEqual(press(start, ' ', ' ').effect, { kind: 'cancel' })
  assert.equal(press(start, ' ', 'ArrowLeft').state.lifted.target, 'none')
})

test('keyboard: Esc with nothing lifted is left to the caller', () => {
  assert.deepEqual(moveSelection(layout, { cursor: null, lifted: null }, 'Escape').effect, { kind: 'escape' })
})

test('cardTitle reads like the History row', () => {
  const t = (k) => ({ 'run.labelSession': 'Session', 'run.labelIssue': 'Issue', 'run.labelPr': 'PR' })[k]
  assert.equal(cardTitle({ title: ' VNPay ', run_type: 'session', ref_number: null }, t), 'VNPay')
  assert.equal(cardTitle({ title: null, run_type: 'session', ref_number: null }, t), 'Session')
  assert.equal(cardTitle({ title: null, run_type: 'analyze_issue', ref_number: 4 }, t), 'Issue #4')
  assert.equal(cardTitle({ title: null, run_type: 'review_pr', ref_number: 9 }, t), 'PR #9')
})

test('resolveBoardProject: saved, else the tab project, else the first, else all', () => {
  assert.equal(resolveBoardProject('all', ['a'], 'a'), 'all')
  assert.equal(resolveBoardProject('b', ['a', 'b'], 'a'), 'b')
  assert.equal(resolveBoardProject('gone', ['a', 'b'], 'b'), 'b')
  assert.equal(resolveBoardProject(null, ['a', 'b'], 'zzz'), 'a')
  assert.equal(resolveBoardProject(null, [], null), 'all')
})

test('card status: a pending question wins, a fresh session is a draft', () => {
  assert.equal(cardStatus({ status: 'done', run_type: 'session' }, true), 'waiting')
  assert.equal(cardStatus({ status: 'fetched', run_type: 'session' }, false), 'draft')
  assert.equal(cardStatus({ status: 'failed', run_type: 'session' }, false), 'failed')
  assert.equal(cardStatus({ status: 'running', run_type: 'session' }, false), 'running')
})
