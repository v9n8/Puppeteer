// Unit tests for lib/dockMode.ts (Active runs dock: full vs compact, per window height).
// Run: node --test src/lib/__tests__/dockMode.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  DEFAULT_DOCK_PREFS, SHORT_HEIGHT, dockModeFor, parseDockPrefs, toggledDockPrefs,
} from '../dockMode.ts'

test('defaults: full when tall, compact below 800', () => {
  assert.equal(dockModeFor(DEFAULT_DOCK_PREFS, 900), 'full')
  assert.equal(dockModeFor(DEFAULT_DOCK_PREFS, SHORT_HEIGHT), 'full')
  assert.equal(dockModeFor(DEFAULT_DOCK_PREFS, SHORT_HEIGHT - 1), 'compact')
  assert.equal(dockModeFor(DEFAULT_DOCK_PREFS, 600), 'compact')
})

test('the two heights remember their choice separately', () => {
  const expandedShort = toggledDockPrefs(DEFAULT_DOCK_PREFS, 600)
  assert.deepEqual(expandedShort, { tall: 'full', short: 'full' })
  assert.equal(dockModeFor(expandedShort, 600), 'full')
  const collapsedTall = toggledDockPrefs(expandedShort, 900)
  assert.deepEqual(collapsedTall, { tall: 'compact', short: 'full' })
  assert.equal(dockModeFor(collapsedTall, 600), 'full')
  assert.equal(dockModeFor(collapsedTall, 900), 'compact')
})

test('toggling twice returns to the start', () => {
  assert.deepEqual(toggledDockPrefs(toggledDockPrefs(DEFAULT_DOCK_PREFS, 700), 700), DEFAULT_DOCK_PREFS)
})

test('parse: round trip, missing, corrupt, partial, wrong types', () => {
  const stored = { tall: 'compact', short: 'full' }
  assert.deepEqual(parseDockPrefs(JSON.stringify(stored)), stored)
  assert.deepEqual(parseDockPrefs(null), DEFAULT_DOCK_PREFS)
  assert.deepEqual(parseDockPrefs('{nope'), DEFAULT_DOCK_PREFS)
  assert.deepEqual(parseDockPrefs('null'), DEFAULT_DOCK_PREFS)
  assert.deepEqual(parseDockPrefs('{"tall":"compact"}'), { tall: 'compact', short: 'compact' })
  assert.deepEqual(parseDockPrefs('{"tall":3,"short":"x"}'), DEFAULT_DOCK_PREFS)
})

test('parse returns a fresh object, not the shared default', () => {
  assert.notEqual(parseDockPrefs(null), DEFAULT_DOCK_PREFS)
})
