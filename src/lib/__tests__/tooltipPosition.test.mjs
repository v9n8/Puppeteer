// Unit tests for lib/tooltipPosition.ts: where InfoTip's tooltip sits (UX-33).
// Run: node --test src/lib/__tests__/tooltipPosition.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { placeTooltip, TOOLTIP_EDGE, TOOLTIP_GAP } from '../tooltipPosition.ts'

const viewport = { width: 1200, height: 800 }
const tip = { width: 300, height: 80 }
const anchorAt = (left, top) => ({ left, top, width: 14, height: 14 })

test('fits above: centred on the anchor, above it', () => {
  const p = placeTooltip(anchorAt(500, 400), tip, viewport)
  assert.equal(p.side, 'top')
  assert.equal(p.top, 400 - TOOLTIP_GAP - 80)
  assert.equal(p.left, 500 + 7 - 150)
})

test('no room above: flips below the anchor', () => {
  const p = placeTooltip(anchorAt(500, 20), tip, viewport)
  assert.equal(p.side, 'bottom')
  assert.equal(p.top, 20 + 14 + TOOLTIP_GAP)
})

test('near the left edge: slides right to stay inside the window', () => {
  const p = placeTooltip(anchorAt(4, 400), tip, viewport)
  assert.equal(p.left, TOOLTIP_EDGE)
})

test('near the right edge: slides left to stay inside the window', () => {
  const p = placeTooltip(anchorAt(1190, 400), tip, viewport)
  assert.equal(p.left, 1200 - 300 - TOOLTIP_EDGE)
})

test('short window: picks the roomier side and clamps inside it', () => {
  const small = { width: 400, height: 120 }
  // 30px above, ~70px below: below is roomier, and the tip is clamped to the bottom edge.
  const p = placeTooltip(anchorAt(100, 30), { width: 200, height: 100 }, small)
  assert.equal(p.side, 'bottom')
  assert.equal(p.top, 120 - 100 - TOOLTIP_EDGE)
  assert.ok(p.top >= TOOLTIP_EDGE)
})

test('tip larger than the window: pinned to the top-left edge, never negative', () => {
  const p = placeTooltip(anchorAt(50, 50), { width: 500, height: 300 }, { width: 300, height: 200 })
  assert.equal(p.left, TOOLTIP_EDGE)
  assert.equal(p.top, TOOLTIP_EDGE)
})
