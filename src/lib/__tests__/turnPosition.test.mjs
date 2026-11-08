// Unit tests for lib/turnPosition.ts (prompt navigator counter + "Top" button).
// Run: node --test src/lib/__tests__/turnPosition.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { isAwayFromTop, promptsBefore, turnCounterLabel, turnIndexAt } from '../turnPosition.ts'

// Tops relative to the container's visible top: scrollTop shifts every turn up.
const at = (offsets, scrollTop) => (i) => offsets[i] - scrollTop
const OFFSETS = [16, 400, 900, 2000, 2050]
const ANCHOR = 16

test('turnIndexAt: above the first turn is -1', () => {
  assert.equal(turnIndexAt(2, at([100, 400], 0), ANCHOR), -1)
})

test('turnIndexAt: no turns is -1', () => {
  assert.equal(turnIndexAt(0, () => 0, ANCHOR), -1)
})

test('turnIndexAt: the turn parked at the anchor line counts as current', () => {
  assert.equal(turnIndexAt(OFFSETS.length, at(OFFSETS, 0), ANCHOR), 0)
  assert.equal(turnIndexAt(OFFSETS.length, at(OFFSETS, 900 - ANCHOR), ANCHOR), 2)
})

test('turnIndexAt: between two turns picks the one above', () => {
  assert.equal(turnIndexAt(OFFSETS.length, at(OFFSETS, 600), ANCHOR), 1)
  assert.equal(turnIndexAt(OFFSETS.length, at(OFFSETS, 1500), ANCHOR), 2)
})

test('turnIndexAt: scrolled past the last turn picks the last', () => {
  assert.equal(turnIndexAt(OFFSETS.length, at(OFFSETS, 99999), ANCHOR), OFFSETS.length - 1)
})

test('turnIndexAt: matches a linear scan on every scroll position', () => {
  const offsets = Array.from({ length: 500 }, (_, i) => 20 + i * 137)
  for (let scrollTop = 0; scrollTop < 500 * 137 + 200; scrollTop += 53) {
    let linear = -1
    for (let i = 0; i < offsets.length; i++) {
      if (offsets[i] - scrollTop > ANCHOR) break
      linear = i
    }
    assert.equal(turnIndexAt(offsets.length, at(offsets, scrollTop), ANCHOR), linear)
  }
})

test('turnIndexAt: reads only a few tops on a long list', () => {
  const offsets = Array.from({ length: 500 }, (_, i) => i * 100)
  let reads = 0
  turnIndexAt(offsets.length, (i) => {
    reads++
    return offsets[i] - 25000
  }, ANCHOR)
  assert.ok(reads <= 10, `read ${reads} tops`)
})

test('turnCounterLabel: n/N, 1-based', () => {
  assert.equal(turnCounterLabel(0, 12), '1/12')
  assert.equal(turnCounterLabel(11, 12), '12/12')
})

test('turnCounterLabel: above the first prompt shows –/N', () => {
  assert.equal(turnCounterLabel(-1, 12), '–/12')
})

test('isAwayFromTop: window that starts later is always away', () => {
  assert.equal(isAwayFromTop(true, 12), true)
  assert.equal(isAwayFromTop(true, null), true)
})

test('isAwayFromTop: landed on the first prompt is at the top', () => {
  assert.equal(isAwayFromTop(false, 12), false)
  assert.equal(isAwayFromTop(false, 0), false)
  assert.equal(isAwayFromTop(false, null), false)
})

test('isAwayFromTop: first prompt scrolled past the top edge is away', () => {
  assert.equal(isAwayFromTop(false, -1), true)
  assert.equal(isAwayFromTop(false, -5000), true)
})

test('promptsBefore: a window starting at a prompt begins with that prompt', () => {
  assert.equal(promptsBefore([0, 100, 250], 100), 1)
  assert.equal(promptsBefore([0, 100, 250], 0), 0)
})

test('promptsBefore: a window starting mid-turn begins with the next prompt', () => {
  assert.equal(promptsBefore([0, 100, 250], 180), 2)
  assert.equal(promptsBefore([0, 100, 250], 900), 3)
})

test('promptsBefore: prompts without a known offset are not counted', () => {
  assert.equal(promptsBefore([undefined, undefined], 500), 0)
  assert.equal(promptsBefore([0, undefined, 250], 300), 2)
})
