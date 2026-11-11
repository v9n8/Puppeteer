// Unit tests for lib/autoGrow.ts (text fields that grow and can be sized by hand).
// Run: node --test src/lib/__tests__/autoGrow.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { autoGrowHeight, clampDragHeight } from '../autoGrow.ts'

test('without a dragged height: grows with the text up to the cap', () => {
  assert.equal(autoGrowHeight(60, 180), 60)
  assert.equal(autoGrowHeight(400, 180), 180)
})

test('a dragged height is a floor, and may pass the cap', () => {
  assert.equal(autoGrowHeight(60, 180, 120), 120)
  assert.equal(autoGrowHeight(150, 180, 120), 150)
  assert.equal(autoGrowHeight(400, 180, 300), 300)
  assert.equal(autoGrowHeight(400, 180, 100), 180)
})

test('clampDragHeight keeps a drag within its bounds', () => {
  assert.equal(clampDragHeight(10, 44, 600), 44)
  assert.equal(clampDragHeight(900.4, 44, 600), 600)
  assert.equal(clampDragHeight(250.6, 44, 600), 251)
  assert.equal(clampDragHeight(250, 44, 20), 44)
})
