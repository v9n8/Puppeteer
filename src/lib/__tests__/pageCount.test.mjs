// Unit tests for lib/pageCount.ts.
// Run: node --test src/lib/__tests__/pageCount.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { pageCount } from '../pageCount.ts'

test('pageCount hides the pill before the first load and on error', () => {
  assert.equal(pageCount(false, null, 0), null)
  assert.equal(pageCount(false, null, 5), null)
  assert.equal(pageCount(true, 'boom', 5), null)
})

test('pageCount shows the count, including 0 for a loaded empty list', () => {
  assert.equal(pageCount(true, null, 3), 3)
  assert.equal(pageCount(true, null, 0), 0)
})
