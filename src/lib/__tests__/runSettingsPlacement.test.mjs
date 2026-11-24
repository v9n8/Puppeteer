// Unit tests for lib/runSettingsPlacement.ts (where the teleported Run options panel sits).
// Run: node --test src/lib/__tests__/runSettingsPlacement.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { placeRunSettingsPanel } from '../runSettingsPlacement.ts'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const viewport = { width: 1280, height: 800 }
const panel = { width: 288, height: 300 }

test('centred above the button with an 8px gap when there is room', () => {
  const anchor = { left: 400, right: 440, top: 700 }
  assert.deepEqual(placeRunSettingsPanel(anchor, panel, viewport), { left: 276, top: 392 })
})

test('a button near the left edge keeps the panel 8px inside the window', () => {
  const anchor = { left: 90, right: 130, top: 700 }
  assert.equal(placeRunSettingsPanel(anchor, panel, viewport).left, 8)
})

test('a button near the right edge keeps the panel 8px inside the window', () => {
  const anchor = { left: 1240, right: 1270, top: 700 }
  assert.equal(placeRunSettingsPanel(anchor, panel, viewport).left, 1280 - 288 - 8)
})

test('without room above the button the panel top stays 8px from the window top', () => {
  const anchor = { left: 400, right: 440, top: 200 }
  assert.equal(placeRunSettingsPanel(anchor, panel, viewport).top, 8)
})

test('a panel larger than the window keeps its left and top edges visible', () => {
  const anchor = { left: 400, right: 440, top: 700 }
  const big = { width: 2000, height: 1000 }
  assert.deepEqual(placeRunSettingsPanel(anchor, big, viewport), { left: 8, top: 8 })
})

test('the panel is teleported out of the composer and outside clicks recognise it', () => {
  const composer = read('../../components/sessions/RunComposer.vue')
  const teleport = composer.indexOf('<Teleport to="body">')
  const marker = composer.indexOf('data-run-settings-panel')
  assert.ok(teleport !== -1 && marker > teleport)
  assert.match(composer.slice(marker, composer.indexOf('>', marker)), /role="dialog"/)
  const hook = read('../../components/sessions/useRunSettingsPanel.ts')
  assert.match(hook, /\[data-run-settings-panel\]/)
  assert.match(hook, /\[data-app-select-dropdown\]/)
})
