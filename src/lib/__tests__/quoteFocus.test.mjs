// RT-19 (#3): a Quote taken in the same window lands in a focused composer —
// from a file tab of the Session, and from the fullscreen message view (which
// closes so the composer is visible). A separate file window keeps the focus.
// Run: node --test src/lib/__tests__/quoteFocus.test.mjs
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

test('quoteAppend carries a focus flag to the composer', () => {
  const src = read('../quoteAppend.ts')
  assert.match(src, /focus\?: boolean/)
  assert.match(src, /append: \(text: string, source: QuoteSource, note\?: string, focus\?: boolean\) => void/)
  assert.match(src, /export async function sendQuote\(runId: string, text: string, source: QuoteSource, note\?: string, focus = false\)/)
  assert.match(src, /composer\.append\(q\.text, q\.source, q\.note, q\.focus\)/)
})

test('the file tab asks for focus, the file window does not', () => {
  assert.match(read('../../components/sessions/FileTabView.vue'), /sendQuote\(run\.id, text, source, note, true\)/)
  assert.doesNotMatch(read('../../views/FileViewerWindow.vue'), /sendQuote\([^)]*true\)/)
})

test('takeQuote focuses the composer only when asked', () => {
  const src = read('../../components/sessions/useRunComposer.ts')
  const start = src.indexOf('function takeQuote(')
  const body = src.slice(start, src.indexOf('\n  }\n', start))
  assert.match(body, /focus\?: boolean/)
  assert.match(body, /if \(focus\) focusComposer\(\)/)
})

test('Quote from the fullscreen view closes it, so the focused composer shows', () => {
  const src = read('../../components/sessions/MessageFullscreen.vue')
  assert.match(src, /append: \(text, anchor, note\) => \{\s*target\.append\(text, anchor, note\)\s*emit\('close'\)/)
})
