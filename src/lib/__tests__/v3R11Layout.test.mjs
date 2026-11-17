// Source guards for the V3-R11 layout fixes (V3-05 AC-3, V3-28 AC-8/AC-10, V3-34 AC-12/AC-14).
// Run: node --test src/lib/__tests__/v3R11Layout.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const src = new URL('../../', import.meta.url)
const read = (p) => readFileSync(new URL(p, src), 'utf8')

test('V3-05 AC-3: the code copy button is at least 28px (1.75rem)', () => {
  const css = read('assets/main.css')
  const rule = css.match(/\.code-copy-btn \{[^}]*\}/)[0]
  assert.match(rule, /height: 1\.75rem/)
  assert.match(rule, /width: 1\.75rem/)
})

test('V3-28 AC-8/10: translate popover picks the language in one select, close button is 28px and named', () => {
  const vue = read('components/TranslatePopover.vue')
  assert.doesNotMatch(vue, /v-for="opt in LANG_OPTIONS"/)
  assert.match(vue, /<AppSelect/)
  const close = vue.match(/<button[^>]*?>(?:(?!<\/button>)[\s\S])*?<X /)
  assert.ok(close, 'close button not found')
  assert.match(close[0], /h-7 w-7/)
  assert.match(close[0], /:aria-label="t\('misc\.translate\.close'\)"/)
})

test('V3-34 AC-14: the composer toolbar row does not wrap', () => {
  const vue = read('components/sessions/RunComposer.vue')
  assert.doesNotMatch(vue, /flex flex-wrap items-center gap-1\.5 px-1\.5 pb-1\.5/)
  assert.match(vue, /<RunComposerChips/)
})

test('V3-34 AC-12: the engine switch dialog names engines with their display name', () => {
  const vue = read('components/sessions/RunEngineSwitchDialog.vue')
  assert.doesNotMatch(vue, /\{\{ handoffTarget \}\}/)
  assert.doesNotMatch(vue, /\{\{ loadedRunEngine \|\| t\('run\.currentEngine'\) \}\}/)
  assert.match(vue, /\{\{ targetName \}\}/)
  assert.match(vue, /\{\{ fromName \}\}/)
})
