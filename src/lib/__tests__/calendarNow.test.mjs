// Unit tests for lib/calendarNow.ts (now line, ongoing events, Calendar keys, per-account merge).
// Run: node --test src/lib/__tests__/calendarNow.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  calendarKeyAction, isOngoing, mergeAccountEvents, minutesLeft, nowLineTop,
} from '../calendarNow.ts'

const at = (h, m = 0) => new Date(2026, 10, 17, h, m, 0, 0)
const ev = (s, e) => ({ start: s.toISOString(), end: e.toISOString(), all_day: false })

test('nowLineTop places the time proportionally to the hour height', () => {
  assert.equal(nowLineTop(at(14, 20), 0, 60), 14 * 60 + 20)
  assert.ok(Math.abs(nowLineTop(at(14, 30), 0, 44) - 14.5 * 44) < 1e-9)
  assert.equal(nowLineTop(at(9, 0), 8, 44), 44)
})

test('isOngoing includes start, excludes end, ignores all-day', () => {
  const e = ev(at(10), at(11))
  assert.equal(isOngoing(e, at(9, 59)), false)
  assert.equal(isOngoing(e, at(10)), true)
  assert.equal(isOngoing(e, at(10, 59)), true)
  assert.equal(isOngoing(e, at(11)), false)
  assert.equal(isOngoing({ start: '2026-11-17', end: '2026-11-18', all_day: true }, at(10)), false)
})

test('minutesLeft rounds up and is 0 outside the event', () => {
  const e = ev(at(10), at(11))
  assert.equal(minutesLeft(e, at(10, 40)), 20)
  assert.equal(minutesLeft(e, new Date(2026, 10, 17, 10, 40, 30)), 20)
  assert.equal(minutesLeft(e, at(11)), 0)
  assert.equal(minutesLeft(e, at(9)), 0)
})

test('calendarKeyAction maps the Calendar keys', () => {
  const k = key => calendarKeyAction({ key, target: { closest: () => null } })
  assert.equal(k('ArrowLeft'), 'prev')
  assert.equal(k('ArrowRight'), 'next')
  assert.equal(k('t'), 'today')
  assert.equal(k('T'), 'today')
  assert.equal(k('w'), 'week')
  assert.equal(k('m'), 'month')
  assert.equal(k('n'), 'create')
  assert.equal(k('x'), null)
  assert.equal(k('Enter'), null)
})

test('calendarKeyAction ignores modifiers, IME, inputs and dialogs', () => {
  const free = { closest: () => null }
  for (const mod of ['metaKey', 'ctrlKey', 'altKey', 'isComposing', 'defaultPrevented']) {
    assert.equal(calendarKeyAction({ key: 't', [mod]: true, target: free }), null)
  }
  assert.equal(calendarKeyAction({ key: 't', target: { closest: () => ({}) } }), null)
  assert.equal(calendarKeyAction({ key: 't', target: { isContentEditable: true } }), null)
  assert.equal(calendarKeyAction({ key: 't', target: null }), 'today')
})

test('mergeAccountEvents replaces only the retried account', () => {
  const events = [
    { id: 'a1', account_id: 'A' }, { id: 'b1', account_id: 'B' }, { id: 'a2', account_id: 'A' },
  ]
  const fresh = [{ id: 'a3', account_id: 'A' }, { id: 'zz', account_id: 'B' }]
  assert.deepEqual(mergeAccountEvents(events, 'A', fresh).map(e => e.id), ['b1', 'a3'])
  assert.deepEqual(mergeAccountEvents(events, 'A', []).map(e => e.id), ['b1'])
})
