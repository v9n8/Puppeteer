// Unit tests for formatTokensShort in lib/contextLimits.ts (V3-16 AC-3: "82k/200k", not "82.0k/200.0k").
// Run: node --test src/lib/__tests__/contextLimits.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { formatTokensShort } from '../contextLimits.ts'

test('round thousands drop the ".0"', () => {
  assert.equal(formatTokensShort(82_000), '82k')
  assert.equal(formatTokensShort(200_000), '200k')
  assert.equal(formatTokensShort(41_000), '41k')
})

test('fractions keep one decimal', () => {
  assert.equal(formatTokensShort(45_200), '45.2k')
  assert.equal(formatTokensShort(1_500_000), '1.5M')
})

test('round millions and small counts', () => {
  assert.equal(formatTokensShort(1_000_000), '1M')
  assert.equal(formatTokensShort(999), '999')
})
