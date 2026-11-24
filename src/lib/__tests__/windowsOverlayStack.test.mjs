// Source checks of the "Open windows" layer in the dialog stack (ZI-05,
// components/windows/WindowsOverlay.vue): the key rules lib/modalStack's own test cannot see.
// Run: node --test src/lib/__tests__/windowsOverlayStack.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const overlay = readFileSync(new URL('../../components/windows/WindowsOverlay.vue', import.meta.url), 'utf8')
const onKeydown = overlay.match(/function onKeydown\(e: KeyboardEvent\) \{[\s\S]*?\n\}/)[0]

test('ZI-05: under a dialog the layer takes no key and drops the focus it still holds', () => {
  assert.match(
    onKeydown,
    /if \(!isTopModal\(layerId\)\) \{\s*if \(e\.target instanceof HTMLElement && root\.value\?\.contains\(e\.target\)\) e\.target\.blur\(\)\s*return\s*\}/,
  )
  // After the open shortcut and the closed check, before every key of the layer itself.
  const at = (s) => onKeydown.indexOf(s)
  assert.ok(at('if (!store.open) return') < at('if (!isTopModal(layerId))'))
  assert.ok(at('store.openOrCycle()') < at('if (!isTopModal(layerId))'))
  assert.ok(at('if (!isTopModal(layerId))') < at('if (drag.value || selectBox.value) return'))
})

test('ZI-05: the Esc that may close the layer stops there, so a dialog under it stays open', () => {
  assert.match(
    onKeydown,
    /if \(e\.key === 'Escape'\) \{\s*e\.preventDefault\(\)\s*(?:\/\/[^\n]*\n\s*)*e\.stopPropagation\(\)\s*(?:\/\/[^\n]*\n\s*)*if \(marks\.value\.size\) clearMarks\(\)/,
  )
})
