// Unit tests for lib/quotaNow.ts (the "Now: …" line of Settings › Limits).
// Run: node --test src/lib/__tests__/quotaNow.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { quotaNowLine } from '../quotaNow.ts'

const NOW = Date.parse('2026-11-10T12:00:00Z')
const FRESH = '2026-11-10T11:58:00Z'
const w = (key, percent, extra = {}) => ({
  key, period: key === 'five_hour' ? '5h' : 'week', percent, reset: null, rolledOver: false, blocked: false, ...extra,
})
const row = (label, windows, capturedAt = FRESH) => ({ label, windows, capturedAt })

test('two Claude accounts and Codex: 5h lists those with a 5h window, weekly lists all', () => {
  const rows = [
    row('Work', [w('five_hour', 62), w('seven_day', 30)]),
    row('Personal', [w('five_hour', 41), w('seven_day', 20), w('seven_day_opus', 55)]),
    row('Codex', [w('seven_day', 12)]),
  ]
  assert.deepEqual(quotaNowLine(rows, '5h', NOW).map((e) => [e.label, e.percent, e.stale]), [
    ['Work', 62, false], ['Personal', 41, false],
  ])
  assert.deepEqual(quotaNowLine(rows, 'week', NOW).map((e) => [e.label, e.percent]), [
    ['Work', 30], ['Personal', 55], ['Codex', 12],
  ])
})

test('an account without a snapshot is skipped', () => {
  const rows = [row('Work', [w('five_hour', 10)]), row('New', [], null)]
  assert.deepEqual(quotaNowLine(rows, '5h', NOW).map((e) => e.label), ['Work'])
})

test('an old capture or a rolled-over window is marked stale, with its capture time', () => {
  const rows = [
    row('Old', [w('five_hour', 80)], '2026-11-10T08:00:00Z'),
    row('Rolled', [w('five_hour', 0, { rolledOver: true })]),
    row('NoTime', [w('five_hour', 5)], null),
  ]
  const out = quotaNowLine(rows, '5h', NOW)
  assert.deepEqual(out.map((e) => [e.label, e.stale]), [['Old', true], ['Rolled', true], ['NoTime', true]])
  assert.equal(out[0].capturedAt, '2026-11-10T08:00:00Z')
})

test('nobody has a figure: empty list', () => {
  assert.deepEqual(quotaNowLine([row('A', []), row('B', [], null)], 'week', NOW), [])
  assert.deepEqual(quotaNowLine([], '5h', NOW), [])
})
