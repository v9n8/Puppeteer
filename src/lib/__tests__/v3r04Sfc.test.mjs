// Source checks for V3-R04 fixes in the Session components (no component harness here).
// Run: node --test src/lib/__tests__/v3r04Sfc.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (f) => readFileSync(new URL(`../../components/sessions/${f}`, import.meta.url), 'utf8')

test('F-R2-3: Quote block measures the Session frame, not the composer cell', () => {
  assert.match(read('SessionSplitView.vue'), /<div ref="grid" data-run-frame /)
  const q = read('ComposerQuoteBlock.vue')
  assert.match(q, /closest<HTMLElement>\('\[data-run-frame\]'\)/)
  assert.doesNotMatch(q, /composer\??\.parentElement/)
})

test('F-U1-R2-8: header keeps the space before the notes count', () => {
  assert.match(read('ComposerQuoteBlock.vue'), /<template v-if="withNotes">\{\{ ' ' \}\}/)
})

test('F-R2-2: the cut mark is skipped by page find', () => {
  assert.match(read('TurnCutMark.vue'), /class="turn-cut-mark search-skip /)
})

test('F-R2-4 / F-U1-R2-1: title edit resets on Session change; button is 32px', () => {
  const s = read('SessionTitle.vue')
  assert.match(s, /watch\(\(\) => currentRun\.value\?\.id, cancel\)/)
  assert.match(s, /rounded-sm border border-transparent px-1.5 text-left/)
})

test('F-U1-R2-2: Esc is heard on the document and removed on unmount', () => {
  const s = read('TurnStatusBar.vue')
  assert.match(s, /document\.addEventListener\('keydown', onKeydown\)/)
  assert.match(s, /document\.removeEventListener\('keydown', onKeydown\)/)
  assert.doesNotMatch(s, /@keydown="onKeydown"/)
})
