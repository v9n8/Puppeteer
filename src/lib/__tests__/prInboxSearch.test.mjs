// Unit tests for lib/prInboxSearch.ts (draft vs applied search, clear button,
// ⌘F mapping). Run: node --test src/lib/__tests__/prInboxSearch.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { applySearch, canClear, clearSearch, isPrSearchShortcut, searchState } from '../prInboxSearch.ts'
import { emptyFilters } from '../prInboxFilters.ts'

test('typing leaves the applied text alone; apply copies it (trimmed)', () => {
  const typed = { ...searchState(), draft: '  gantt ' }
  assert.equal(typed.applied, '')
  assert.deepEqual(applySearch(typed), { draft: '  gantt ', applied: 'gantt' })
})

test('applying an empty draft lifts the text filter', () => {
  const s = applySearch({ draft: '', applied: 'gantt' })
  assert.equal(s.applied, '')
})

test('clearSearch (Esc / x) empties both draft and applied', () => {
  assert.deepEqual(clearSearch(), { draft: '', applied: '' })
})

test('canClear: text or filters, nothing else', () => {
  const none = emptyFilters()
  assert.equal(canClear(searchState(), none), false)
  assert.equal(canClear({ draft: '', applied: 'x' }, none), true)
  assert.equal(canClear({ draft: 'x', applied: '' }, none), true)
  assert.equal(canClear(searchState(), { ...none, authors: ['a'] }), true)
})

test('isPrSearchShortcut: Cmd+F on mac, Ctrl+F elsewhere, never with Shift', () => {
  assert.equal(isPrSearchShortcut({ key: 'f', metaKey: true, ctrlKey: false }, true), true)
  assert.equal(isPrSearchShortcut({ key: 'f', metaKey: false, ctrlKey: true }, true), false)
  assert.equal(isPrSearchShortcut({ key: 'F', metaKey: false, ctrlKey: true }, false), true)
  assert.equal(isPrSearchShortcut({ key: 'f', metaKey: true, ctrlKey: false, shiftKey: true }, true), false)
})
