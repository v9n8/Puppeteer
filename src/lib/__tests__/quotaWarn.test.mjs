// Unit tests for lib/quotaWarn.ts (quota warning toast: once per window crossing).
// Run: node --test src/lib/__tests__/quotaWarn.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { takeQuotaWarnCrossings } from '../quotaWarn.ts'

const w = (period, percent) => ({ period, percent, reset: '2026-11-10T15:00:00Z' })
const ids = (list) => list.map((c) => `${c.key}|${c.period}|${c.percent}`)

test('a window warns once while it stays in the warning tier', () => {
  const latch = new Set()
  assert.deepEqual(ids(takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: [w('5h', 81)] }])), ['codex|5h|81'])
  assert.deepEqual(takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: [w('5h', 84)] }]), [])
  // A moved reset time is still the same window.
  assert.deepEqual(takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: [{ period: '5h', percent: 86, reset: null }] }]), [])
})

test('5h and weekly windows warn separately, per row', () => {
  const latch = new Set()
  assert.deepEqual(ids(takeQuotaWarnCrossings(latch, [
    { key: 'claude:a1', windows: [w('5h', 90)] },
    { key: 'claude:a2', windows: [w('5h', 82)] },
  ])), ['claude:a1|5h|90', 'claude:a2|5h|82'])
  assert.deepEqual(ids(takeQuotaWarnCrossings(latch, [
    { key: 'claude:a1', windows: [w('5h', 91), w('week', 80)] },
    { key: 'claude:a2', windows: [w('5h', 83)] },
  ])), ['claude:a1|week|80'])
})

test('dropping below (or a reset) re-arms the window', () => {
  const latch = new Set()
  takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: [w('5h', 81), w('week', 85)] }])
  assert.deepEqual(takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: [w('week', 85)] }]), [])
  assert.deepEqual(ids(takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: [w('5h', 80), w('week', 86)] }])), ['codex|5h|80'])
})

test('a row without a status keeps its latch; the tier switched off clears it', () => {
  const latch = new Set()
  takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: [w('5h', 81)] }])
  assert.deepEqual(takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: undefined }]), [])
  assert.deepEqual(takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: [w('5h', 81)] }]), [], 'not repeated')
  takeQuotaWarnCrossings(latch, [{ key: 'codex', windows: [] }])
  assert.equal(latch.size, 0)
})

test('one row never clears another row whose key shares a prefix', () => {
  const latch = new Set()
  takeQuotaWarnCrossings(latch, [{ key: 'claude', windows: [w('5h', 81)] }, { key: 'claude:a1', windows: [w('5h', 81)] }])
  takeQuotaWarnCrossings(latch, [{ key: 'claude:a1', windows: [] }])
  assert.deepEqual([...latch], ['claude|5h'])
})
