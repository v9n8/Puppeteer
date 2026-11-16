// Unit tests for lib/sidebarMode.ts (full sidebar vs icon rail, per window size).
// Run: node --test src/lib/__tests__/sidebarMode.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  DEFAULT_PREFS, NARROW_WIDTH, parseSidebarPrefs, sidebarModeFor, toggledPrefs, withMode,
} from '../sidebarMode.ts'

test('defaults: full when wide, rail below 1100', () => {
  assert.equal(sidebarModeFor(DEFAULT_PREFS, 1366), 'full')
  assert.equal(sidebarModeFor(DEFAULT_PREFS, NARROW_WIDTH), 'full')
  assert.equal(sidebarModeFor(DEFAULT_PREFS, NARROW_WIDTH - 1), 'rail')
  assert.equal(sidebarModeFor(DEFAULT_PREFS, 900), 'rail')
})

test('toggling at a narrow size leaves the wide choice alone', () => {
  const next = toggledPrefs(DEFAULT_PREFS, 1000)
  assert.deepEqual(next, { wide: 'full', narrow: 'full' })
  assert.equal(sidebarModeFor(next, 1366), 'full')
  assert.deepEqual(toggledPrefs(next, 1366), { wide: 'rail', narrow: 'full' })
})

test('withMode sets only the current size', () => {
  assert.deepEqual(withMode(DEFAULT_PREFS, 1000, 'full'), { wide: 'full', narrow: 'full' })
  assert.deepEqual(withMode(DEFAULT_PREFS, 1400, 'rail'), { wide: 'rail', narrow: 'rail' })
  assert.deepEqual(withMode(DEFAULT_PREFS, 1400, 'full'), DEFAULT_PREFS)
})

test('the old hidden key becomes rail at both sizes', () => {
  assert.deepEqual(parseSidebarPrefs(null, '1'), { wide: 'rail', narrow: 'rail' })
  assert.deepEqual(parseSidebarPrefs(null, '0'), DEFAULT_PREFS)
  assert.deepEqual(parseSidebarPrefs(null, null), DEFAULT_PREFS)
})

test('the new key wins over the old one', () => {
  assert.deepEqual(parseSidebarPrefs('{"wide":"full","narrow":"full"}', '1'), { wide: 'full', narrow: 'full' })
})

test('broken JSON or bad fields fall back to the defaults', () => {
  assert.deepEqual(parseSidebarPrefs('{nope', '1'), DEFAULT_PREFS)
  assert.deepEqual(parseSidebarPrefs('null'), DEFAULT_PREFS)
  assert.deepEqual(parseSidebarPrefs('{"wide":"rail","narrow":"huge"}'), { wide: 'rail', narrow: 'rail' })
  assert.deepEqual(parseSidebarPrefs('{"wide":5}'), DEFAULT_PREFS)
})
