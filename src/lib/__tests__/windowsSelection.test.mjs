// Unit tests for lib/windowsSelection.ts: marking several cards of the "Open windows"
// overlay and the plan of moving them back to the main window.
// Run: node --test src/lib/__tests__/windowsSelection.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { markAll, mergePlan, pruneMarks, rangeMark, toggleMark } from '../windowsSelection.ts'

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
