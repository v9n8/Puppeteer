// Unit tests for lib/pageFind.ts (the in-page ⌘F search).
// Run: node --test src/lib/__tests__/pageFind.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  FIND_IN_PAGE_FEATURE,
  MAX_MATCHES,
  findMatches,
  isFindShortcut,
  stepMatch,
} from '../pageFind.ts'

test('findMatches: empty query matches nothing', () => {
  assert.deepEqual(findMatches(['abc'], ''), [])
})

test('findMatches: no chunks matches nothing', () => {
  assert.deepEqual(findMatches([], 'a'), [])
})

test('findMatches: positions across chunks, in order', () => {
  assert.deepEqual(findMatches(['foo bar', 'nothing', 'barbar'], 'bar'), [
    { chunk: 0, start: 4, end: 7 },
    { chunk: 2, start: 0, end: 3 },
    { chunk: 2, start: 3, end: 6 },
  ])
})

test('findMatches: ignores case by default', () => {
  assert.deepEqual(findMatches(['Hello HELLO hello'], 'hello'), [
    { chunk: 0, start: 0, end: 5 },
    { chunk: 0, start: 6, end: 11 },
    { chunk: 0, start: 12, end: 17 },
  ])
  assert.equal(findMatches(['abc'], 'ABC').length, 1)
})

test('findMatches: caseSensitive keeps only the exact case', () => {
  assert.deepEqual(findMatches(['Hello HELLO hello'], 'hello', { caseSensitive: true }), [
    { chunk: 0, start: 12, end: 17 },
  ])
  assert.deepEqual(findMatches(['abc'], 'ABC', { caseSensitive: true }), [])
})

test('findMatches: matches do not overlap', () => {
  assert.deepEqual(findMatches(['aaaa'], 'aa'), [
    { chunk: 0, start: 0, end: 2 },
    { chunk: 0, start: 2, end: 4 },
  ])
})

test('findMatches: a match never spans two chunks', () => {
  assert.deepEqual(findMatches(['foo', 'bar'], 'ob'), [])
})

test('findMatches: stops at the limit', () => {
  const m = findMatches(['x x x', 'x x'], 'x', { limit: 4 })
  assert.equal(m.length, 4)
  assert.deepEqual(m.at(-1), { chunk: 1, start: 0, end: 1 })
})

test('findMatches: default limit is MAX_MATCHES (5000)', () => {
  assert.equal(MAX_MATCHES, 5000)
  assert.equal(findMatches(['a'.repeat(6000)], 'a').length, 5000)
})

test('findMatches: a query with regex characters is matched literally', () => {
  assert.deepEqual(findMatches(['a.b a*b'], '.'), [{ chunk: 0, start: 1, end: 2 }])
})

test('stepMatch: next and previous wrap around', () => {
  assert.equal(stepMatch(1, 3, 1), 2)
  assert.equal(stepMatch(3, 3, 1), 1)
  assert.equal(stepMatch(1, 3, -1), 3)
  assert.equal(stepMatch(2, 3, -1), 1)
})

test('stepMatch: no matches stays at 0', () => {
  assert.equal(stepMatch(0, 0, 1), 0)
  assert.equal(stepMatch(0, 0, -1), 0)
})

test('stepMatch: a delta larger than the count still wraps', () => {
  assert.equal(stepMatch(1, 3, 4), 2)
  assert.equal(stepMatch(1, 3, -4), 3)
})

test('isFindShortcut: Cmd+F and Ctrl+F, either case', () => {
  assert.equal(isFindShortcut({ key: 'f', metaKey: true, ctrlKey: false }), true)
  assert.equal(isFindShortcut({ key: 'F', metaKey: false, ctrlKey: true }), true)
})

test('isFindShortcut: plain F or another key is not', () => {
  assert.equal(isFindShortcut({ key: 'f', metaKey: false, ctrlKey: false }), false)
  assert.equal(isFindShortcut({ key: 'g', metaKey: true, ctrlKey: false }), false)
})

test('FIND_IN_PAGE_FEATURE is the registry prop id', () => {
  assert.equal(FIND_IN_PAGE_FEATURE, 'text_tools.find_in_page')
})
