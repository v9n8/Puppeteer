// Unit tests for lib/latestRead.ts (RT-15: a usage ring never goes back to an
// older number when reads of the same key come back out of order).
// Run: node --test src/lib/__tests__/latestRead.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createLatestRead } from '../latestRead.ts'

const later = (ms, value) => new Promise((resolve) => setTimeout(() => resolve(value), ms))

test('a read that comes back after a newer one of the same key is dropped', async () => {
  const latest = createLatestRead()
  const shown = {}
  const load = async (key, ms, value) => {
    const current = latest.begin(key)
    const got = await later(ms, value)
    if (current()) shown[key] = got
  }
  // Refresh spam: the first read is slow and returns the older snapshot.
  await Promise.all([load('claude:a1', 30, 'old 34%'), load('claude:a1', 5, 'new 40%')])
  assert.equal(shown['claude:a1'], 'new 40%')
})

test('keys are ordered independently', async () => {
  const latest = createLatestRead()
  const a = latest.begin('claude')
  const b = latest.begin('codex')
  assert.equal(a(), true)
  assert.equal(b(), true)
  const a2 = latest.begin('claude')
  assert.equal(a(), false)
  assert.equal(a2(), true)
  assert.equal(b(), true)
})
