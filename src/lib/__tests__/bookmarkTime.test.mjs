// Unit tests for lib/bookmarkTime.ts (when a bookmark was placed).
// Run: node --test src/lib/__tests__/bookmarkTime.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { bookmarkWhen } from '../bookmarkTime.ts'

// Local-time constructors keep the tests independent of the machine's zone.
const at = (y, mo, d, h, mi) => new Date(y, mo - 1, d, h, mi).toISOString()
const now = new Date(2026, 10, 16, 15, 0)

test('same day is today', () => {
  assert.deepEqual(bookmarkWhen(at(2026, 11, 16, 10, 41), now, 'en'), { time: '10:41', day: 'today' })
})

test('previous day is yesterday, even across midnight', () => {
  assert.deepEqual(bookmarkWhen(at(2026, 11, 15, 9, 12), now, 'vi'), { time: '09:12', day: 'yesterday' })
  const early = new Date(2026, 10, 16, 0, 5)
  assert.equal(bookmarkWhen(at(2026, 11, 15, 23, 59), early, 'en').day, 'yesterday')
})

test('older this year is a date without the year', () => {
  const w = bookmarkWhen(at(2026, 11, 12, 8, 3), now, 'en')
  assert.equal(w.day, 'date')
  assert.equal(w.time, '08:03')
  assert.ok(w.date.includes('12') && !w.date.includes('2026'))
})

test('another year carries the year', () => {
  const w = bookmarkWhen(at(2025, 12, 31, 23, 0), now, 'en')
  assert.equal(w.day, 'date')
  assert.ok(w.date.includes('2025'))
})

test('year boundary: Dec 31 is yesterday on Jan 1', () => {
  const jan1 = new Date(2027, 0, 1, 9, 0)
  assert.equal(bookmarkWhen(at(2026, 12, 31, 20, 0), jan1, 'en').day, 'yesterday')
})

test('locale picks the date format', () => {
  const en = bookmarkWhen(at(2026, 11, 12, 8, 3), now, 'en').date
  const vi = bookmarkWhen(at(2026, 11, 12, 8, 3), now, 'vi').date
  assert.notEqual(en, vi)
})

test('an unparsable date gives null', () => {
  assert.equal(bookmarkWhen('c', now, 'en'), null)
})
