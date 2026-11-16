// Unit tests for lib/secretVaultTimers.ts.
// Run: node --test src/lib/__tests__/secretVaultTimers.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { AUTO_LOCK_CHOICES, parseAutoLockMinutes, revealCountdown } from '../secretVaultTimers.ts'

test('revealCountdown counts whole seconds down and stops at 0', () => {
  assert.equal(revealCountdown(1000, 1000, 60), 60)
  assert.equal(revealCountdown(1000, 1001, 60), 60)
  assert.equal(revealCountdown(1000, 19_000, 60), 42)
  assert.equal(revealCountdown(1000, 60_999, 60), 1)
  assert.equal(revealCountdown(1000, 61_000, 60), 0)
  assert.equal(revealCountdown(1000, 99_000, 60), 0)
})

test('the auto-lock menu is 1, 5, 15, 30 minutes', () => {
  assert.deepEqual([...AUTO_LOCK_CHOICES], [1, 5, 15, 30])
})

test('parseAutoLockMinutes keeps menu values and falls back to 5', () => {
  assert.equal(parseAutoLockMinutes(1), 1)
  assert.equal(parseAutoLockMinutes('15'), 15)
  assert.equal(parseAutoLockMinutes(' 30 '), 30)
  for (const bad of [undefined, null, '', 'abc', 0, 2, 61, -1, 1.5, NaN, '7', {}]) {
    assert.equal(parseAutoLockMinutes(bad), 5, String(bad))
  }
})
