// Unit tests for lib/pageFind.ts (the in-page ⌘F search).
// Run: node --test src/lib/__tests__/pageFind.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  FIND_IN_PAGE_FEATURE,
  MAX_MATCHES,
  findMatches,
  isFindShortcut,
  pickFindArea,
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

test('isFindShortcut: Cmd+F on macOS, Ctrl+F elsewhere, either case', () => {
  assert.equal(isFindShortcut({ key: 'f', metaKey: true, ctrlKey: false }, true), true)
  assert.equal(isFindShortcut({ key: 'F', metaKey: true, ctrlKey: false }, true), true)
  assert.equal(isFindShortcut({ key: 'f', metaKey: false, ctrlKey: true }, false), true)
  assert.equal(isFindShortcut({ key: 'F', metaKey: false, ctrlKey: true }, false), true)
})

test('isFindShortcut: Ctrl+F on macOS is not (cursor forward in a text field)', () => {
  assert.equal(isFindShortcut({ key: 'f', metaKey: false, ctrlKey: true }, true), false)
})

test('isFindShortcut: plain F or another key is not', () => {
  for (const isMac of [true, false]) {
    assert.equal(isFindShortcut({ key: 'f', metaKey: false, ctrlKey: false }, isMac), false)
    assert.equal(isFindShortcut({ key: 'g', metaKey: isMac, ctrlKey: !isMac }, isMac), false)
  }
})

test('FIND_IN_PAGE_FEATURE is the registry prop id', () => {
  assert.equal(FIND_IN_PAGE_FEATURE, 'text_tools.find_in_page')
})

// ── Which area ⌘F goes to ───────────────────────────────────────────────────

/** A fake DOM node: `contains` walks up the parent chain (self included). */
function node(name, parent = null) {
  const n = {
    name,
    parent,
    contains(other) {
      for (let x = other; x; x = x.parent) if (x === n) return true
      return false
    },
  }
  return n
}

const body = node('body')
const runRoot = node('run', body)
const runText = node('runText', runRoot)
const composer = node('composer', body)
const viewerRoot = node('viewer', body)
const viewerText = node('viewerText', viewerRoot)
const backdrop = node('backdrop', body)
const modalRoot = node('modalFind', backdrop)
const modalText = node('modalText', modalRoot)

const run = { root: runRoot }
const viewer = { root: viewerRoot }
const inModal = { root: modalRoot }
const hidden = { root: null }
const rootOf = (a) => a.root
const ctx = (over = {}) => ({ excludedWindow: false, overlay: null, hints: [], ...over })

test('pickFindArea: the area holding the focus wins', () => {
  assert.equal(pickFindArea([run, viewer], rootOf, ctx({ hints: [runText] })), run)
  assert.equal(pickFindArea([run, viewer], rootOf, ctx({ hints: [viewerText] })), viewer)
})

test('pickFindArea: focus outside every area falls to the last click', () => {
  assert.equal(pickFindArea([run, viewer], rootOf, ctx({ hints: [composer, runText] })), run)
  assert.equal(pickFindArea([run, viewer], rootOf, ctx({ hints: [null, viewerText] })), viewer)
})

test('pickFindArea: no hint in an area picks the last mounted one', () => {
  assert.equal(pickFindArea([run, viewer], rootOf, ctx({ hints: [composer, body] })), viewer)
  assert.equal(pickFindArea([run], rootOf, ctx()), run)
})

test('pickFindArea: an area off screen (null root) never takes ⌘F', () => {
  assert.equal(pickFindArea([run, hidden], rootOf, ctx()), run)
  assert.equal(pickFindArea([hidden], rootOf, ctx()), null)
  assert.equal(pickFindArea([], rootOf, ctx()), null)
})

test('pickFindArea: nested areas — the innermost holding the hint wins', () => {
  const inner = { root: node('inner', runRoot) }
  const innerText = node('innerText', inner.root)
  assert.equal(pickFindArea([inner, run], rootOf, ctx({ hints: [innerText] })), inner)
  assert.equal(pickFindArea([inner, run], rootOf, ctx({ hints: [runText] })), run)
})

test('pickFindArea: under an open overlay only an area inside it takes ⌘F', () => {
  const overlay = backdrop
  assert.equal(pickFindArea([run, inModal], rootOf, ctx({ overlay, hints: [runText] })), inModal)
  assert.equal(pickFindArea([inModal, run], rootOf, ctx({ overlay })), inModal)
  assert.equal(pickFindArea([inModal], rootOf, ctx({ overlay, hints: [modalText] })), inModal)
})

test('pickFindArea: an overlay with no area blocks the page behind it', () => {
  const overlay = node('fullscreenMessage', body)
  assert.equal(pickFindArea([run, viewer], rootOf, ctx({ overlay, hints: [runText] })), null)
  assert.equal(pickFindArea([run], rootOf, ctx({ overlay })), null)
})

test('pickFindArea: the Secret vault window never gets ⌘F', () => {
  assert.equal(pickFindArea([run], rootOf, ctx({ excludedWindow: true, hints: [runText] })), null)
  assert.equal(pickFindArea([run, viewer], rootOf, ctx({ excludedWindow: true })), null)
})
