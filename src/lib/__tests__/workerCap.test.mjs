// Unit tests for lib/workerCap.ts (QO-03): the default worker cap of a new session.
// Run: node --test src/lib/__tests__/workerCap.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { defaultWorkerCap } from '../workerCap.ts'

test('an unset or unreadable setting gives 3', () => {
  for (const raw of [undefined, null, '', '  ', 'many', '2.5', '3x']) assert.equal(defaultWorkerCap(raw), 3, String(raw))
})

test('a whole number is kept within 1–100', () => {
  assert.equal(defaultWorkerCap('5'), 5)
  assert.equal(defaultWorkerCap(' 7 '), 7)
  assert.equal(defaultWorkerCap(12), 12)
  assert.equal(defaultWorkerCap('0'), 1)
  assert.equal(defaultWorkerCap('-4'), 1)
  assert.equal(defaultWorkerCap('500'), 100)
})
