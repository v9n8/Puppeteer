// Unit tests for lib/quoteBlockHeight.ts (height of the composer's Quote list).
// Run: node --test src/lib/__tests__/quoteBlockHeight.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { defaultQuoteListHeight, quoteRowsHeight, clampQuoteListHeight, stepQuoteListHeight } from '../quoteBlockHeight.ts'

// The composer list as drawn: rows 34px, a row with a note 55px, 4px apart, 6px padding below.
const box = (rows) => ({ rows, gap: 4, padTop: 0, padBottom: 6 })

test('rows: three quotes (two with notes) fit whole with the bottom padding, so nothing scrolls', () => {
  assert.equal(quoteRowsHeight(box([55, 55, 34])), 55 + 4 + 55 + 4 + 34 + 6)
})

test('rows: more than three show three whole and a peek of the fourth', () => {
  assert.equal(quoteRowsHeight(box([55, 55, 55, 55, 34])), 55 * 3 + 4 * 2 + 4 + 24)
})

test('rows: fewer than three take only what they need', () => {
  assert.equal(quoteRowsHeight(box([34, 55])), 34 + 4 + 55 + 6)
})

test('rows: a fractional total rounds up so the last pixel is not cut', () => {
  assert.equal(quoteRowsHeight({ rows: [34, 34], gap: 4, padTop: 0.5, padBottom: 6 }), 79)
})

test('default: measured rows on a tall window', () => {
  assert.equal(defaultQuoteListHeight(box([55, 55, 34]), 38, 800), 158)
})

test('default: measured rows still capped at half the conversation', () => {
  assert.equal(defaultQuoteListHeight(box([55, 55, 55, 55]), 38, 300), 150)
})

test('default: before the rows are measured, three rows and a peek stand in', () => {
  assert.equal(defaultQuoteListHeight(null, 32, 500), 120)
  assert.equal(defaultQuoteListHeight(box([]), 32, 500), 120)
})

test('default: capped at half the conversation on a short window', () => {
  assert.equal(defaultQuoteListHeight(null, 32, 104), 52)
})

test('default never goes below one row', () => {
  assert.equal(defaultQuoteListHeight(null, 32, 20), 32)
  assert.equal(defaultQuoteListHeight({ rows: [10], gap: 0, padTop: 0, padBottom: 0 }, 38, 500), 38)
})

test('clamp: lowest one row, highest half the conversation', () => {
  assert.equal(clampQuoteListHeight(5, 32, 500), 32)
  assert.equal(clampQuoteListHeight(400, 32, 500), 250)
  assert.equal(clampQuoteListHeight(100.4, 32, 500), 100)
})

test('step: up adds a row, down removes one, both stay in range', () => {
  assert.equal(stepQuoteListHeight(120, 'up', 32, 500), 152)
  assert.equal(stepQuoteListHeight(120, 'down', 32, 500), 88)
  assert.equal(stepQuoteListHeight(240, 'up', 32, 500), 250)
  assert.equal(stepQuoteListHeight(40, 'down', 32, 500), 32)
})
