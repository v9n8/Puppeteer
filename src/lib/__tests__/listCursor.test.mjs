// Unit tests for lib/listCursor.ts (keyboard cursor over search hits and chart days).
// Run: node --test src/lib/__tests__/listCursor.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { flatHits, moveCursor } from '../listCursor.ts'

test('empty list always gives -1', () => {
  for (const k of ['ArrowUp', 'ArrowDown', 'Home', 'End']) assert.equal(moveCursor(-1, 0, k), -1)
})

test('down enters at the first item and stops at the last', () => {
  assert.equal(moveCursor(-1, 3, 'ArrowDown'), 0)
  assert.equal(moveCursor(0, 3, 'ArrowDown'), 1)
  assert.equal(moveCursor(2, 3, 'ArrowDown'), 2)
})

test('up from the first item returns to the box; from the box stays there', () => {
  assert.equal(moveCursor(1, 3, 'ArrowUp'), 0)
  assert.equal(moveCursor(0, 3, 'ArrowUp'), -1)
  assert.equal(moveCursor(-1, 3, 'ArrowUp'), -1)
})

test('Home and End jump to the ends', () => {
  assert.equal(moveCursor(1, 4, 'Home'), 0)
  assert.equal(moveCursor(1, 4, 'End'), 3)
})

test('flatHits walks across group borders in order', () => {
  const groups = [{ hits: [{ offset: 5 }, { offset: 9 }] }, { hits: [] }, { hits: [{ offset: 2 }] }]
  assert.deepEqual(flatHits(groups), [
    { groupIndex: 0, offset: 5 },
    { groupIndex: 0, offset: 9 },
    { groupIndex: 2, offset: 2 },
  ])
  assert.deepEqual(flatHits([]), [])
})
