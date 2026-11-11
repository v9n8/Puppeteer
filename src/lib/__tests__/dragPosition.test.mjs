// Unit tests for lib/dragPosition.ts: a dragged / resized panel (the Quote
// popover, UX-46) stays inside the window.
// Run: node --test src/lib/__tests__/dragPosition.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { clampPosition, clampSize, DRAG_EDGE } from '../dragPosition.ts'

const viewport = { width: 1200, height: 800 }
const panel = { width: 280, height: 200 }
const limits = { minWidth: 240, maxWidth: 560, minHeight: 160 }

test('clampPosition: a panel inside the window stays where it is', () => {
  assert.deepEqual(clampPosition({ x: 100, y: 50 }, panel, viewport), { x: 100, y: 50 })
})

test('clampPosition: dragged past any edge, the panel stops 8px inside it', () => {
  assert.equal(DRAG_EDGE, 8)
  assert.deepEqual(clampPosition({ x: -50, y: -30 }, panel, viewport), { x: 8, y: 8 })
  assert.deepEqual(clampPosition({ x: 1100, y: 750 }, panel, viewport), { x: 1200 - 280 - 8, y: 800 - 200 - 8 })
  assert.deepEqual(clampPosition({ x: 2, y: 790 }, panel, viewport), { x: 8, y: 592 })
})

test('clampPosition: a panel larger than the window pins to its top-left edge', () => {
  assert.deepEqual(clampPosition({ x: 300, y: 300 }, { width: 600, height: 500 }, { width: 400, height: 300 }), { x: 8, y: 8 })
})

test('clampSize: within the width range and the minimum height', () => {
  const at = { x: 100, y: 100 }
  assert.deepEqual(clampSize({ width: 300, height: 250 }, at, viewport, limits), { width: 300, height: 250 })
  assert.deepEqual(clampSize({ width: 100, height: 40 }, at, viewport, limits), { width: 240, height: 160 })
  assert.deepEqual(clampSize({ width: 900, height: 300 }, at, viewport, limits), { width: 560, height: 300 })
})

test('clampSize: never past the right / bottom edge, 8px in', () => {
  const at = { x: 800, y: 500 }
  assert.deepEqual(clampSize({ width: 500, height: 400 }, at, viewport, limits), { width: 1200 - 800 - 8, height: 800 - 500 - 8 })
})

test('clampSize: where the window has less room than the minimum, the minimum wins', () => {
  const at = { x: 1100, y: 750 }
  assert.deepEqual(clampSize({ width: 500, height: 400 }, at, viewport, limits), { width: 240, height: 160 })
})
