// Source checks of the DS-5 round (move private windows back to the main window,
// components/windows/*): the parts a unit test of lib/windowsSelection.ts cannot see.
// Run: node --test src/lib/__tests__/reviewUiV3ds5.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(`../../${p}`, import.meta.url), 'utf8')
const overlay = read('components/windows/WindowsOverlay.vue')
const json = (lang) => JSON.parse(read(`i18n/locales/${lang}/windows.json`))

test('UR-52: "Move all back" uses the merge plan, asks nothing, is off when nothing can move back', () => {
  assert.match(overlay, /mergePlan\(store\.cards, labels, store\.here\)/)
  assert.match(overlay, /:disabled="!canMergeAny"[\s\S]{0,200}@click="mergeAllWindows"/)
  assert.match(overlay, /function mergeAllWindows\(\) \{[^}]*mergeWindows\(store\.cards\.map/)
  // D7: no confirm on the way.
  assert.doesNotMatch(overlay.match(/async function mergeWindows[\s\S]*?\n\}/)[0], /confirm/)
})

test('UR-52: short header labels under 1100px, a narrower filter at 901px', () => {
  assert.match(overlay, /max-\[1100px\]:hidden">\{\{ t\('windows\.mergeAll'\) \}\}/)
  assert.match(overlay, /min-\[1100px\]:hidden">\{\{ t\('windows\.mergeAllShort'\) \}\}/)
  assert.match(overlay, /min-\[1100px\]:hidden">\{\{ t\('windows\.closeAllShort'\) \}\}/)
  assert.match(overlay, /w-\[220px\][^"]*max-\[901px\]:w-\[150px\]/)
})

test('UR-52: the header stays one row with a filter on, the filter shrinks first', () => {
  assert.match(overlay, /<header class="flex items-center gap-2 /)
  assert.match(overlay, /w-\[220px\] min-w-\[150px\]/)
  assert.match(overlay, /<h2 class="whitespace-nowrap /)
})

test('UR-52: toast strings in both languages', () => {
  for (const lang of ['vi', 'en']) {
    const w = json(lang)
    for (const k of ['mergeAll', 'mergeAllShort', 'closeAllShort', 'mergeAllTip', 'merged', 'mergedSkipped']) {
      assert.ok(w[k], `${lang}.${k}`)
    }
    assert.match(w.mergedSkipped, /\{count\}[\s\S]*\{skipped\}/)
  }
})
