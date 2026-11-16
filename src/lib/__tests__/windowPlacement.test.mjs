// Unit tests for the pure rules of lib/windowPlacement.ts (lib/windowPlacementCore.ts):
// where a detachable screen opens and where its own window comes back.
// Run: node --test src/lib/__tests__/windowPlacement.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  DEFAULT_OPEN_MODE, DETACHABLE_SCREENS, PLACEMENT_STORAGE_KEY,
  centerBounds, fitBounds, openModeOf, openModesOf, parsePlacements, serializePlacements,
  withBounds, withOpenMode,
} from '../windowPlacementCore.ts'

const EMPTY = { local: {}, remote: {} }
const MAIN = { x: 0, y: 0, width: 1440, height: 900 }
const NO_MIN = { width: 0, height: 0 }

test('the storage key is the one the issue fixed', () => {
  assert.equal(PLACEMENT_STORAGE_KEY, 'puppeteer.windowPlacement')
})

test('parsePlacements: missing, broken or foreign values give empty sets', () => {
  assert.deepEqual(parsePlacements(null), EMPTY)
  assert.deepEqual(parsePlacements(''), EMPTY)
  assert.deepEqual(parsePlacements('{bad'), EMPTY)
  assert.deepEqual(parsePlacements('[]'), EMPTY)
  assert.deepEqual(parsePlacements('"x"'), EMPTY)
  assert.deepEqual(parsePlacements('{"local":{"calendar":{"mode":"window"}}}'), EMPTY, 'no version')
  assert.deepEqual(parsePlacements('{"v":2,"local":{"calendar":{"mode":"window"}}}'), EMPTY, 'other version')
})

test('parsePlacements keeps valid entries and drops unknown keys and modes', () => {
  const raw = JSON.stringify({
    v: 1,
    local: {
      calendar: { mode: 'window', bounds: { x: -1200, y: 40, width: 1000.4, height: 700 } },
      tasks: { mode: 'floating' },
      bogus: { mode: 'window' },
      session: { mode: 'window', bounds: { x: 1, y: 2, width: 800, height: 600 } },
    },
    remote: { gantt: { mode: 'main' } },
  })
  assert.deepEqual(parsePlacements(raw), {
    local: {
      calendar: { mode: 'window', bounds: { x: -1200, y: 40, width: 1000, height: 700 } },
      // A session window has no mode, only bounds.
      session: { bounds: { x: 1, y: 2, width: 800, height: 600 } },
    },
    remote: { gantt: { mode: 'main' } },
  })
})

test('parsePlacements drops bounds with a negative / zero size, NaN or strings', () => {
  const bad = [
    { x: 0, y: 0, width: -10, height: 500 },
    { x: 0, y: 0, width: 800, height: 0 },
    { x: 'a', y: 0, width: 800, height: 600 },
    { x: 0, y: 0, width: 800 },
    null,
  ]
  for (const bounds of bad) {
    const raw = JSON.stringify({ v: 1, local: { gantt: { mode: 'window', bounds } } })
    assert.deepEqual(parsePlacements(raw).local, { gantt: { mode: 'window' } }, JSON.stringify(bounds))
  }
  // NaN / Infinity cannot be written as JSON; they arrive as null and are dropped too.
  const raw = serializePlacements({ local: { gantt: { bounds: { x: NaN, y: 0, width: Infinity, height: 1 } } }, remote: {} })
  assert.deepEqual(parsePlacements(raw).local, {})
})

test('serializePlacements round-trips through parsePlacements', () => {
  const all = {
    local: { 'pr-inbox': { mode: 'window', bounds: { x: 10, y: 20, width: 1100, height: 780 } } },
    remote: { calendar: { bounds: { x: 0, y: 0, width: 900, height: 600 } } },
  }
  assert.deepEqual(parsePlacements(serializePlacements(all)), all)
})

test('openModeOf falls back to the default; secret notes and file transfer default to their own window', () => {
  assert.equal(openModeOf({}, 'calendar'), 'main')
  assert.equal(openModeOf({}, 'secret-notes'), 'window')
  assert.equal(openModeOf({}, 'file-transfer'), 'window')
  assert.equal(openModeOf({ calendar: { mode: 'window' } }, 'calendar'), 'window')
  assert.equal(openModeOf({ 'secret-notes': { mode: 'main' } }, 'secret-notes'), 'main')
  // Only bounds saved: the mode is still the default.
  assert.equal(openModeOf({ gantt: { bounds: MAIN } }, 'gantt'), 'main')
})

test('openModesOf lists every detachable screen', () => {
  const modes = openModesOf({ tasks: { mode: 'window' } })
  assert.deepEqual(Object.keys(modes).sort(), [...DETACHABLE_SCREENS].sort())
  assert.equal(modes.tasks, 'window')
  assert.equal(modes['file-transfer'], DEFAULT_OPEN_MODE['file-transfer'])
})

test('withOpenMode / withBounds return a new map and keep the other field', () => {
  const bounds = { x: 1, y: 2, width: 3, height: 4 }
  const before = { calendar: { bounds } }
  const frozen = JSON.stringify(before)
  const a = withOpenMode(before, 'calendar', 'window')
  assert.deepEqual(a, { calendar: { mode: 'window', bounds } })
  assert.equal(JSON.stringify(before), frozen)
  assert.notEqual(a, before)

  const nb = { x: 5, y: 6, width: 7, height: 8 }
  const b = withBounds(a, 'calendar', nb)
  assert.deepEqual(b, { calendar: { mode: 'window', bounds: nb } })
  assert.deepEqual(a.calendar.bounds, bounds)
  // The stored bounds are a copy: changing the argument later changes nothing.
  nb.x = 99
  assert.equal(b.calendar.bounds.x, 5)
})

test('fitBounds keeps a window that is fully on a monitor', () => {
  const b = { x: 100, y: 80, width: 1200, height: 820 }
  assert.deepEqual(fitBounds(b, [MAIN], NO_MIN), b)
})

test('fitBounds returns null when the monitor the window was on is unplugged', () => {
  const onSecond = { x: 1600, y: 100, width: 1200, height: 820 }
  assert.equal(fitBounds(onSecond, [MAIN], NO_MIN), null)
  assert.equal(fitBounds(onSecond, [], NO_MIN), null)
})

test('fitBounds needs at least 64 × 32 px of the title bar on a screen', () => {
  // Only 40 px of width left on the main monitor.
  assert.equal(fitBounds({ x: 1400, y: 100, width: 800, height: 600 }, [MAIN], NO_MIN), null)
  // Title bar above the top of the screen.
  assert.equal(fitBounds({ x: 100, y: -20, width: 800, height: 600 }, [MAIN], NO_MIN), null)
  // Windows parks a minimised window far off-screen.
  assert.equal(fitBounds({ x: -32000, y: -32000, width: 160, height: 28 }, [MAIN], NO_MIN), null)
  // 64 px of title bar is enough; the window is then moved fully on screen.
  assert.deepEqual(
    fitBounds({ x: 1376, y: 100, width: 800, height: 600 }, [MAIN], NO_MIN),
    { x: 640, y: 100, width: 800, height: 600 },
  )
})

test('fitBounds shrinks a window larger than its monitor, never below min unless the monitor is smaller', () => {
  const small = { x: 0, y: 0, width: 1280, height: 720 }
  assert.deepEqual(
    fitBounds({ x: 10, y: 10, width: 1600, height: 1000 }, [small], { width: 760, height: 520 }),
    { x: 0, y: 0, width: 1280, height: 720 },
  )
  assert.deepEqual(
    fitBounds({ x: 10, y: 10, width: 300, height: 200 }, [small], { width: 760, height: 520 }),
    { x: 10, y: 10, width: 760, height: 520 },
  )
})

test('fitBounds works with a monitor at negative coordinates and picks the one holding the title bar', () => {
  const left = { x: -1920, y: -200, width: 1920, height: 1080 }
  const b = { x: -1800, y: -150, width: 1200, height: 800 }
  assert.deepEqual(fitBounds(b, [MAIN, left], NO_MIN), b)
  // Straddling both: most of the title bar is on the left monitor.
  const straddle = { x: -1000, y: 50, width: 1200, height: 600 }
  assert.deepEqual(fitBounds(straddle, [MAIN, left], NO_MIN), { x: -1200, y: 50, width: 1200, height: 600 })
})

test('centerBounds centres the saved size on a monitor, shrinking it to fit', () => {
  assert.deepEqual(centerBounds({ width: 1200, height: 820 }, MAIN, NO_MIN), { x: 120, y: 40, width: 1200, height: 820 })
  const second = { x: -1280, y: 0, width: 1280, height: 720 }
  assert.deepEqual(centerBounds({ width: 1600, height: 1000 }, second, NO_MIN), second)
})
