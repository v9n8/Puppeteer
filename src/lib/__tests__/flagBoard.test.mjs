// Unit tests for lib/flagBoard.ts (V3-22).
// Run: node --test src/lib/__tests__/flagBoard.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  boardColumns, canDrop, cardStatus, cardTitle, dropFlagId, filterCards, groupCards, listSpan, moveSelection, planGroupDrop,
  resolveBoardProject, NO_FLAG_COLUMN,
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

test('all projects: every shared flag, then each own flag as a column that takes a drop (UR-60)', () => {
  const cols = boardColumns([flag('a'), flag('b', { hidden: true }), flag('mine', { project_id: 'p1' })], 'all')
  assert.deepEqual(ids(cols), [NO_FLAG_COLUMN, 'a', 'b', 'mine'])
  assert.ok(cols.every((c) => c.droppable))
})

test('cards go to their flag column; a card whose column is not on the board sits nowhere (UR-60)', () => {
  const cards = [
    { id: '1', flag_id: 'a' }, { id: '2', flag_id: null }, { id: '3', flag_id: 'b' }, { id: '4', flag_id: 'mine' },
  ]
  const one = groupCards(cards, boardColumns(flags, 'project'))
  assert.deepEqual(one.get('a').map((c) => c.id), ['1'])
  assert.deepEqual(one.get(NO_FLAG_COLUMN).map((c) => c.id), ['2'])
  assert.deepEqual([...one.values()].flat().map((c) => c.id).sort(), ['1', '2', '4'])
  const all = groupCards(cards, boardColumns(flags, 'all'))
  assert.deepEqual(all.get('mine').map((c) => c.id), ['4'])
  assert.deepEqual(all.get('b').map((c) => c.id), ['3'])
  const noNone = groupCards(cards, boardColumns(flags, 'all').filter((c) => c.id !== NO_FLAG_COLUMN))
  assert.equal([...noNone.values()].flat().length, 3)
})

test('the filter keeps the name matches and counts matched / total', () => {
  const grouped = new Map([['a', [{ t: 'SSO login' }, { t: 'VNPay' }]], ['c', []]])
  const out = filterCards(grouped, ' sso ', (c) => c.t)
  assert.deepEqual(out.get('a'), { cards: [{ t: 'SSO login' }], matched: 1, total: 2 })
  assert.deepEqual(out.get('c'), { cards: [], matched: 0, total: 0 })
  assert.equal(filterCards(grouped, '', (c) => c.t).get('a').matched, 2)
})

test('canDrop: not on a flag the card project hides, own flags only in their project', () => {
  const cols = boardColumns(flags, 'all')
  const col = (id) => cols.find((c) => c.id === id)
  const hidden = [{ project_id: 'p1', flag_id: 'b' }]
  assert.equal(canDrop({ project_id: 'p1' }, col('mine'), hidden), true)
  assert.equal(canDrop({ project_id: 'p2' }, col('mine'), hidden), false)
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

const allCols = boardColumns(flags, 'all')
const allCol = (id) => allCols.find((c) => c.id === id)
const split = (plan) => ({ move: plan.move.map((c) => c.id), blocked: plan.blocked.map((c) => c.id), already: plan.already.map((c) => c.id) })

test('planGroupDrop on a shared flag: others move, the ones there are skipped, a Project that hides it is refused', () => {
  const cards = [
    { id: '1', project_id: 'p2', flag_id: 'c' },
    { id: '2', project_id: 'p2', flag_id: 'a' },
    { id: '3', project_id: 'p1', flag_id: null },
  ]
  const plan = planGroupDrop(cards, allCol('a'), [{ project_id: 'p1', flag_id: 'a' }])
  assert.deepEqual(split(plan), { move: ['1'], blocked: ['3'], already: ['2'] })
})

test('planGroupDrop on a Project\'s own flag: only that Project\'s cards move', () => {
  const cards = [
    { id: '1', project_id: 'p1', flag_id: 'a' },
    { id: '2', project_id: 'p2', flag_id: 'a' },
    { id: '3', project_id: 'p1', flag_id: 'mine' },
  ]
  assert.deepEqual(split(planGroupDrop(cards, allCol('mine'), [])), { move: ['1'], blocked: ['2'], already: ['3'] })
})

test('planGroupDrop on No flag: flagged cards move, a card with no flag (null or missing) is already there', () => {
  const cards = [
    { id: '1', project_id: 'p1', flag_id: 'mine' },
    { id: '2', project_id: 'p2', flag_id: null },
    { id: '3', project_id: 'p2' },
    { id: '4', project_id: 'p2', flag_id: 'b' },
  ]
  assert.deepEqual(split(planGroupDrop(cards, allCol(NO_FLAG_COLUMN), [{ project_id: 'p2', flag_id: 'b' }])), {
    move: ['1', '4'], blocked: [], already: ['2', '3'],
  })
})

test('planGroupDrop keeps the order it was given inside each list', () => {
  const cards = [
    { id: '5', project_id: 'p2', flag_id: 'a' },
    { id: '4', project_id: 'p1', flag_id: null },
    { id: '3', project_id: 'p2', flag_id: 'mine' },
    { id: '2', project_id: 'p1', flag_id: 'a' },
    { id: '1', project_id: 'p1', flag_id: 'mine' },
  ]
  assert.deepEqual(split(planGroupDrop(cards, allCol('mine'), [])), { move: ['4', '2'], blocked: ['5'], already: ['3', '1'] })
})

const band = { top: 100, bottom: 500 }

test('listSpan: a box inside the list covers the part between press and pointer, whichever way it goes', () => {
  assert.deepEqual(listSpan(150, 300, band, 0, 0), { top: 50, bottom: 200 })
  assert.deepEqual(listSpan(300, 150, band, 0, 0), { top: 50, bottom: 200 })
})

test('listSpan: a press on the header of a scrolled list starts at the visible top, not at the cards above it', () => {
  assert.deepEqual(listSpan(40, 200, band, 120, 120), { top: 120, bottom: 220 })
})

test('listSpan: the list scrolling under the box keeps the press end and grows the span', () => {
  assert.deepEqual(listSpan(150, 520, band, 0, 80), { top: 50, bottom: band.bottom - band.top + 80 })
})

test('listSpan: a box wholly above or below the list covers none of it', () => {
  assert.equal(listSpan(20, 80, band, 0, 0), null)
  assert.equal(listSpan(600, 700, band, 30, 30), null)
})
