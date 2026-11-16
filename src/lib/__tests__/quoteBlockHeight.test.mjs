// Unit tests for lib/quoteBlockHeight.ts (height of the composer's Quote list).
// Run: node --test src/lib/__tests__/quoteBlockHeight.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { defaultQuoteListHeight, clampQuoteListHeight, stepQuoteListHeight } from '../quoteBlockHeight.ts'

test('default: three rows and a peek on a tall window', () => {
  assert.equal(defaultQuoteListHeight(32, 500), 120)
})

test('default: capped at half the conversation on a short window', () => {
  assert.equal(defaultQuoteListHeight(32, 104), 52)
})

test('default never goes below one row', () => {
  assert.equal(defaultQuoteListHeight(32, 20), 32)
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
