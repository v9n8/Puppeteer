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
  assert.match(overlay, /w-\[240px\][^"]*max-\[901px\]:w-\[150px\]/)
})

test('UR-52: the header stays one row with a filter on, the filter shrinks first', () => {
  assert.match(overlay, /<header class="flex items-center gap-2 /)
  assert.match(overlay, /w-\[240px\] min-w-\[150px\]/)
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

const cardSrc = read('components/windows/WindowCard.vue')
const barSrc = read('components/windows/WindowsSelectionBar.vue')

test('UR-53: round checkbox top-left, "This window" moved right of it, a mark for windows that cannot move back', () => {
  assert.match(cardSrc, /role="checkbox"[\s\S]{0,200}class="absolute left-1\.5 top-1\.5 z-\[1\] flex h-5 w-5[^"]*rounded-full/)
  assert.match(cardSrc, /marked\s*\? 'bg-primary text-primary-foreground ring-2 ring-card'/)
  assert.match(cardSrc, /markMode \? 'opacity-100' : 'opacity-0 group-hover:opacity-100'/)
  assert.match(cardSrc, /v-if="card\.isHere"\s*class="absolute left-8 top-1\.5/)
  assert.match(cardSrc, /v-if="!card\.canMerge"[\s\S]{0,200}:title="t\('windows\.cannotMerge'\)"/)
  // Marked: a plain click marks, it does not open the window.
  assert.match(cardSrc, /if \(e\.shiftKey \|\| meta \|\| props\.markMode\) emit\('mark'/)
})

test('UR-53: the selection bar takes the footer, 28px buttons; Esc drops the marks first', () => {
  assert.match(overlay, /<WindowsSelectionBar\s+v-if="marks\.size > 0"/)
  assert.match(overlay, /<footer v-else /)
  assert.equal((barSrc.match(/<Button [^>]*size="xs"/g) ?? []).length, 4)
  assert.match(overlay, /if \(marks\.value\.size\) clearMarks\(\)\s*else if \(query\.value\) query\.value = ''\s*else close\(\)/)
  assert.match(overlay, /confirm\.titleSome/)
})

test('UR-53: Space / ⌘A stay text keys while the filter has text; ⌘A marks only the cards on screen', () => {
  assert.match(overlay, /const typingInFilter = e\.target === filterInput\.value && query\.value !== ''/)
  assert.match(overlay, /=== ' ' && !typingInFilter/)
  assert.match(overlay, /'KeyA'[^\n]*!typingInFilter\) \{\s*e\.preventDefault\(\)\s*marks\.value = markAll\(flat\.value\)/)
  for (const lang of ['vi', 'en']) {
    const w = json(lang)
    for (const k of ['count', 'skip', 'merge', 'close', 'clear']) assert.ok(w.sel[k], `${lang}.sel.${k}`)
    for (const k of ['mark', 'markAll', 'clear']) assert.ok(w.foot[k], `${lang}.foot.${k}`)
    assert.ok(w.cannotMerge && w.mark && w.confirm.titleSome, lang)
  }
})

const stripSrc = read('components/windows/WindowsDropStrip.vue')
const dragSrc = read('components/windows/useWindowsDrag.ts')

test('UR-54, UR-68: an 80px dashed strip stuck at the top of the grid; accent while dragging, amber when refused', () => {
  assert.match(overlay, /<template v-else>\s*<WindowsDropStrip /)
  // Same colour as the overlay panel (bg-popover), or the strip's backing shows as a band.
  assert.match(stripSrc, /class="sticky -top-3 [^"]*bg-popover /)
  assert.match(stripSrc, /data-windows-drop\s+class="flex h-20 [^"]*border-2 border-dashed/)
  // The grid's scroll padding clears the strip (12px top padding + 80px), so a card brought into view by the keys is not under it.
  assert.match(overlay, /class="relative min-h-\[160px\] flex-1 scroll-pt-24 overflow-y-auto px-4 pt-3/)
  assert.match(stripSrc, /idle: 'border-input text-muted-foreground'/)
  assert.match(stripSrc, /merge: 'border-primary bg-primary\/10 text-primary'/)
  assert.match(stripSrc, /refused: '[^']*border-warning bg-warning\/10 text-warning'/)
  assert.match(stripSrc, /tone === 'refused' \? Ban : SquareArrowDownLeft/)
  for (const lang of ['vi', 'en']) {
    const d = json(lang).drop
    assert.ok(d.idle && d.one.includes('{name}') && d.many.includes('{count}') && d.refused.includes('{name}'), lang)
  }
})

test('UR-54: pointer drag (no HTML5 drag), a threshold, Esc cancels, the click after a drag is swallowed', () => {
  assert.doesNotMatch(overlay + cardSrc, /draggable="true"|@dragstart/)
  assert.match(dragSrc, /const THRESHOLD = 4/)
  assert.match(dragSrc, /'Escape' \|\| !drag\.value\) return/)
  assert.match(dragSrc, /overStrip\(e\.clientX, e\.clientY\) && dragged\.value\.some\(\(c\) => c\.canMerge\)/)
  assert.match(overlay, /if \(drag\.value \|\| selectBox\.value\) return/)
  assert.match(overlay, /@activate="consumeClick\(\) \|\| activate\(card\.label\)"/)
  assert.match(cardSrc, /dragging \? 'opacity-40' : ''/)
})

const boxSrc = read('components/windows/useWindowsMarquee.ts')

test('UR-73: a box dragged from empty grid space marks the cards it touches; cards still drag to the strip', () => {
  // Starts only off cards, buttons, links and the strip, past the same 4px threshold as a card drag.
  assert.match(boxSrc, /target\.closest\('\[data-window-label\], button, input, a, \[data-windows-drop\]'\)\) return/)
  assert.match(boxSrc, /const THRESHOLD = 4/)
  // Rules from lib/windowsSelection; ⇧ / ⌘ (Ctrl) at the press adds to the old marks.
  assert.match(boxSrc, /options\.setMarks\(boxMarks\(press\.before, boxHits\(cards, box\.value\), press\.additive\)\)/)
  assert.match(boxSrc, /additive: e\.shiftKey \|\| e\.metaKey \|\| e\.ctrlKey/)
  // Esc / pointercancel / blur put the old marks back; the edge scroll starts under the strip.
  assert.match(boxSrc, /if \(press && box\.value\) options\.setMarks\(press\.before\)/)
  assert.match(boxSrc, /window\.addEventListener\('blur', cancel\)/)
  assert.match(boxSrc, /edgeScrollStep\(pointer\.y, Math\.max\(r\.top, strip\?\.bottom \?\? r\.top\), r\.bottom, EDGE_ZONE, EDGE_MAX\)/)
  // The overlay: the grid takes the press, the box sits under the strip (z-[2]), over the cards.
  assert.match(overlay, /ref="scroller"[\s\S]*?@pointerdown="onGridPointerDown"/)
  assert.match(overlay, /data-windows-box\s+class="pointer-events-none absolute z-\[1\] rounded-sm border border-primary bg-primary\/10"/)
  assert.match(overlay, /:boxing="!!selectBox"/)
  assert.match(overlay, /<span>\{\{ t\('windows\.foot\.box'\) \}\}<\/span>/)
  assert.match(barSrc, /<span v-if="boxing" class="text-muted-foreground">· \{\{ t\('windows\.sel\.boxing'\) \}\}<\/span>/)
  assert.match(barSrc, /t\(boxing \? 'windows\.foot\.cancelBox' : 'windows\.foot\.clear'\)/)
  for (const lang of ['vi', 'en']) {
    const w = json(lang)
    assert.ok(w.sel.boxing && w.foot.box && w.foot.cancelBox, lang)
  }
})
