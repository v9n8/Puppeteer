// Unit tests for lib/taskFieldPatch.ts (one edited cell of the task detail panel → its patch).
// Run: node --test src/lib/__tests__/taskFieldPatch.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { aiApplyPatch, actualPatch, dueDatePatch, dueTimePatch, estimatePatch, instantPatch } from '../taskFieldPatch.ts'

const todo = { estimate_min: 120, actual_min: null, due_date: '2026-11-20', due_time: '17:00', snoozed_until: null, remind_at: null }

test('estimate: parses 30m / 2h / 1h30 and sends minutes', () => {
  assert.deepEqual(estimatePatch('30m', todo), { ok: true, patch: { estimate_min: 30 } })
  assert.deepEqual(estimatePatch('1h30', todo), { ok: true, patch: { estimate_min: 90 } })
  assert.deepEqual(estimatePatch('3h', todo), { ok: true, patch: { estimate_min: 180 } })
})

test('estimate: same value sends nothing, empty clears, garbage is rejected', () => {
  assert.deepEqual(estimatePatch('2h', todo), { ok: true, patch: {} })
  assert.deepEqual(estimatePatch('', todo), { ok: true, patch: { estimate_min: null } })
  assert.deepEqual(estimatePatch('abc', todo), { ok: false })
  assert.deepEqual(estimatePatch('1m', todo), { ok: false })
})

test('actual time uses the same format', () => {
  assert.deepEqual(actualPatch('45m', todo), { ok: true, patch: { actual_min: 45 } })
  assert.deepEqual(actualPatch('', todo), { ok: true, patch: {} })
})

test('due date: change, clear (no due_time sent), unchanged', () => {
  assert.deepEqual(dueDatePatch('2026-11-21', todo), { ok: true, patch: { due_date: '2026-11-21' } })
  assert.deepEqual(dueDatePatch('', todo), { ok: true, patch: { due_date: null } })
  assert.deepEqual(dueDatePatch('2026-11-20', todo), { ok: true, patch: {} })
})

test('due time: needs a date, "" clears it', () => {
  assert.deepEqual(dueTimePatch('09:30', todo), { ok: true, patch: { due_time: '09:30' } })
  assert.deepEqual(dueTimePatch('', todo), { ok: true, patch: { due_time: null } })
  assert.deepEqual(dueTimePatch('09:30', { ...todo, due_date: null }), { ok: true, patch: {} })
})

test('reminder: local value becomes RFC3339, clearing sends null, unchanged sends nothing', () => {
  const r = instantPatch('remind_at', '2026-11-20T09:00', todo)
  assert.equal(r.ok, true)
  assert.equal(r.patch.remind_at, new Date(2026, 10, 20, 9, 0).toISOString())
  assert.deepEqual(instantPatch('remind_at', '', todo), { ok: true, patch: {} })
  const set = { ...todo, remind_at: new Date(2026, 10, 20, 9, 0).toISOString() }
  assert.deepEqual(instantPatch('remind_at', '', set), { ok: true, patch: { remind_at: null } })
  assert.deepEqual(instantPatch('remind_at', '2026-11-20T09:00', set), { ok: true, patch: {} })
  assert.deepEqual(instantPatch('snoozed_until', 'nonsense', todo), { ok: false })
})

test('AI apply: estimate is stored as ai, priority and deadline ride along', () => {
  assert.deepEqual(aiApplyPatch({ estimateMin: 144 }), { estimate_min: 144, estimate_source: 'ai' })
  assert.deepEqual(aiApplyPatch({ priority: 1 }), { priority: 1 })
  assert.deepEqual(aiApplyPatch({ dueDate: '2026-11-22', dueTime: '' }), { due_date: '2026-11-22', due_time: null })
  assert.deepEqual(aiApplyPatch({}), {})
})
