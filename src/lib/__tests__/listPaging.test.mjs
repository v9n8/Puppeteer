// Unit tests for lib/listPaging.ts (the session History's pages).
// Run: node --test src/lib/__tests__/listPaging.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { pageOfIndex, pageSlice, totalPages } from '../listPaging.ts'

const rows = Array.from({ length: 45 }, (_, i) => i)

test('pages of 20: the slice, its 1-based number and the page count', () => {
  assert.deepEqual(pageSlice(rows, 1).items, rows.slice(0, 20))
  assert.deepEqual(pageSlice(rows, 3), { items: [40, 41, 42, 43, 44], page: 3, totalPages: 3 })
})

test('a page out of range shows the nearest one; an empty list is page 1 of 1', () => {
  assert.equal(pageSlice(rows, 9).page, 3)
  assert.equal(pageSlice(rows, 0).page, 1)
  assert.equal(pageSlice(rows, Number.NaN).page, 1)
  assert.deepEqual(pageSlice([], 2), { items: [], page: 1, totalPages: 1 })
  assert.equal(totalPages(0), 1)
  assert.equal(totalPages(40), 2)
})

test('pageOfIndex: the page of a row, null when it is not listed', () => {
  assert.equal(pageOfIndex(0), 1)
  assert.equal(pageOfIndex(19), 1)
  assert.equal(pageOfIndex(20), 2)
  assert.equal(pageOfIndex(-1), null)
})
