// Unit tests for lib/windowsSelection.ts: marking several cards of the "Open windows"
// overlay and the plan of moving them back to the main window.
// Run: node --test src/lib/__tests__/windowsSelection.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  boxBetween, boxHits, boxMarks, edgeScrollStep, markAll, mergePlan, pruneMarks, rangeMark, toggleMark,
} from '../windowsSelection.ts'

const flat = ['a', 'b', 'c', 'd', 'e'].map((label) => ({ label }))
const sorted = (set) => [...set].sort()

test('toggleMark marks, then unmarks, without touching the set it was given', () => {
  const start = new Set(['a'])
  const on = toggleMark(start, 'b')
  assert.deepEqual(sorted(on), ['a', 'b'])
  assert.deepEqual(sorted(start), ['a'])
  assert.deepEqual(sorted(toggleMark(on, 'a')), ['b'])
})

test('rangeMark: forward and backward from the anchor, keeping the marks already there', () => {
  assert.deepEqual(sorted(rangeMark(new Set(), flat, 'b', 'd')), ['b', 'c', 'd'])
  assert.deepEqual(sorted(rangeMark(new Set(['e']), flat, 'd', 'a')), ['a', 'b', 'c', 'd', 'e'])
})

test('rangeMark: no anchor, or an anchor filtered out, marks only the card', () => {
  assert.deepEqual(sorted(rangeMark(new Set(), flat, null, 'c')), ['c'])
  assert.deepEqual(sorted(rangeMark(new Set(), flat, 'zz', 'c')), ['c'])
  // A card not on screen is not marked.
  assert.deepEqual(sorted(rangeMark(new Set(['a']), flat, 'a', 'zz')), ['a'])
})

test('markAll marks only the cards on screen (the filter applies)', () => {
  const filtered = flat.filter((c) => c.label === 'b' || c.label === 'd')
  assert.deepEqual(sorted(markAll(filtered)), ['b', 'd'])
  assert.deepEqual(sorted(markAll([])), [])
})

test('pruneMarks drops the cards closed or filtered out', () => {
  assert.deepEqual(sorted(pruneMarks(new Set(['a', 'c', 'gone']), flat.slice(1))), ['c'])
})

test('mergePlan skips the windows that cannot move back, keeps the order', () => {
  const cards = [
    { label: 'session-1', canMerge: true },
    { label: 'remote-desktop', canMerge: false },
    { label: 'gantt', canMerge: true },
    { label: 'item-list-todo', canMerge: false },
  ]
  assert.deepEqual(
    mergePlan(cards, ['gantt', 'remote-desktop', 'session-1', 'item-list-todo', 'closed-already'], null),
    { merge: ['gantt', 'session-1'], skip: ['remote-desktop', 'item-list-todo'] },
  )
})

test('mergePlan: the window running the overlay moves back last', () => {
  const cards = [
    { label: 'calendar', canMerge: true },
    { label: 'session-1', canMerge: true },
    { label: 'session-2', canMerge: true },
  ]
  assert.deepEqual(mergePlan(cards, ['calendar', 'session-1', 'session-2'], 'session-1').merge, ['calendar', 'session-2', 'session-1'])
  // `here` that cannot move back is skipped like any other.
  assert.deepEqual(mergePlan([{ label: 'remote-desktop', canMerge: false }], ['remote-desktop'], 'remote-desktop'), {
    merge: [], skip: ['remote-desktop'],
  })
})

// Selection box (UR-72): touching a card marks it; ⇧ / ⌘ adds to the marks from before.
const r = (left, top, w = 100, h = 80) => ({ left, top, right: left + w, bottom: top + h })
const grid = [
  { label: 'a', rect: r(0, 0) }, { label: 'b', rect: r(112, 0) }, { label: 'c', rect: r(224, 0) },
  { label: 'd', rect: r(0, 92) }, { label: 'e', rect: r(112, 92) },
]

test('boxBetween: the same box whichever way the pointer went', () => {
  const want = { left: 10, top: 20, right: 150, bottom: 120 }
  assert.deepEqual(boxBetween(10, 20, 150, 120), want)
  assert.deepEqual(boxBetween(150, 120, 10, 20), want)
  assert.deepEqual(boxBetween(150, 20, 10, 120), want)
})

test('boxHits: a card the box only touches counts; one in the gap does not', () => {
  // From the gap left of b down into e: b and e, not a / d (their right edge is at 100).
  assert.deepEqual(boxHits(grid, boxBetween(106, -6, 130, 100)), ['b', 'e'])
  // An edge on an edge counts.
  assert.deepEqual(boxHits(grid, boxBetween(100, 40, 105, 41)), ['a'])
  // Only gap: nothing.
  assert.deepEqual(boxHits(grid, boxBetween(101, 81, 111, 91)), [])
  // Across both rows and every column, in the order given.
  assert.deepEqual(boxHits(grid, boxBetween(-5, -5, 400, 200)), ['a', 'b', 'c', 'd', 'e'])
})

test('boxMarks: plain replaces the marks; additive keeps the old ones even when the box leaves', () => {
  const before = new Set(['a', 'c'])
  assert.deepEqual(sorted(boxMarks(before, ['b', 'e'], false)), ['b', 'e'])
  assert.deepEqual(sorted(boxMarks(before, ['b', 'e'], true)), ['a', 'b', 'c', 'e'])
  // The box shrank off "e" and never covered "a": "a" stays (before), "e" goes.
  assert.deepEqual(sorted(boxMarks(before, ['b'], true)), ['a', 'b', 'c'])
  assert.deepEqual(sorted(boxMarks(before, [], false)), [])
  assert.deepEqual(sorted(before), ['a', 'c'])
})

test('edgeScrollStep: 0 away from the edges, faster nearer the edge, capped, sign = direction', () => {
  const step = (y) => edgeScrollStep(y, 100, 500, 32, 16)
  assert.equal(step(300), 0)
  assert.equal(step(132), 0)
  assert.equal(step(468), 0)
  assert.ok(step(120) < 0 && step(120) > step(101), `${step(120)} ${step(101)}`)
  assert.equal(step(90), -16)
  assert.ok(step(480) > 0 && step(480) < step(499), `${step(480)} ${step(499)}`)
  assert.equal(step(600), 16)
})
