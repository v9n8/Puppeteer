// Unit tests for lib/calendarAllDay.ts.
// Run: node --test src/lib/__tests__/calendarAllDay.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { allDayByDay } from '../calendarAllDay.ts'

const d = (day) => new Date(2026, 10, day)
const week = [16, 17, 18, 19, 20, 21, 22].map(d)
const ad = (id, s, e) => ({ id, allDay: true, start: d(s), end: d(e) })
const ids = (r) => r.map(x => x.map(i => i.id).join(','))

test('single-day event lands in one cell', () => {
  assert.deepEqual(ids(allDayByDay([ad('a', 18, 18)], week)), ['', '', 'a', '', '', '', ''])
})
test('multi-day event covers each day', () => {
  assert.deepEqual(ids(allDayByDay([ad('a', 17, 19)], week)), ['', 'a', 'a', 'a', '', '', ''])
})
test('starting before the week', () => {
  assert.deepEqual(ids(allDayByDay([ad('a', 10, 17)], week)), ['a', 'a', '', '', '', '', ''])
})
test('ending after the week', () => {
  assert.deepEqual(ids(allDayByDay([ad('a', 21, 30)], week)), ['', '', '', '', '', 'a', 'a'])
})
test('outside the week goes nowhere', () => {
  assert.deepEqual(ids(allDayByDay([ad('a', 1, 5), ad('b', 25, 26)], week)), ['', '', '', '', '', '', ''])
})
test('timed events are ignored', () => {
  const timed = { id: 't', allDay: false, start: d(18), end: d(18) }
  assert.deepEqual(ids(allDayByDay([timed], week)), ['', '', '', '', '', '', ''])
})
test('same-day events keep input order', () => {
  assert.equal(ids(allDayByDay([ad('b', 18, 18), ad('a', 18, 18)], week))[2], 'b,a')
})
