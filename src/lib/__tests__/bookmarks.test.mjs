// Unit tests for lib/bookmarks.ts (bookmarks on the turns of a session).
// Run: node --test src/lib/__tests__/bookmarks.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { bookmarkKey, placeBookmarks } from '../bookmarks.ts'

const mark = (id, log_offset, label, side = 'question') => ({ id, run_id: 'r', log_offset, side, label, created_at: 'c' })
const placed = (bookmarks, prompts) => placeBookmarks(bookmarks, prompts).map((p) => [p.bookmark.id, p.seq])

const prompts = [
  { label: 'fix login', offset: 0 },
  { label: 'add tests', offset: 900 },
  { label: 'fix login', offset: 2400 },
]

test('placeBookmarks: a bookmark lands on the prompt at its offset with its label', () => {
  assert.deepEqual(placed([mark('b', 2400, 'fix login'), mark('a', 900, 'add tests')], prompts), [
    ['a', 1],
    ['b', 2],
  ])
})

test('placeBookmarks: offsets moved (log rebuilt) → the same label nearest the old offset', () => {
  const rebuilt = [
    { label: 'fix login', offset: 0 },
    { label: 'add tests', offset: 700 },
    { label: 'fix login', offset: 2000 },
  ]
  assert.deepEqual(placed([mark('a', 900, 'add tests'), mark('b', 2400, 'fix login')], rebuilt), [
    ['a', 1],
    ['b', 2],
  ])
})

test('placeBookmarks: a bookmark whose turn is gone, or prompts not scanned yet, is left out', () => {
  assert.deepEqual(placed([mark('a', 900, 'deleted turn')], prompts), [])
  assert.deepEqual(placed([mark('a', 900, 'add tests')], [{ label: 'add tests' }]), [])
  assert.deepEqual(placed([mark('a', 900, 'add tests')], []), [])
})

test('placeBookmarks: sorted by turn, the question before the answer', () => {
  const marks = [mark('a2', 900, 'add tests', 'answer'), mark('q3', 2400, 'fix login'), mark('q2', 900, 'add tests')]
  assert.deepEqual(placed(marks, prompts), [
    ['q2', 1],
    ['a2', 1],
    ['q3', 2],
  ])
})

test('bookmarkKey: one key per turn and side', () => {
  assert.equal(bookmarkKey(3, 'answer'), '3:answer')
  assert.notEqual(bookmarkKey(3, 'answer'), bookmarkKey(3, 'question'))
})
