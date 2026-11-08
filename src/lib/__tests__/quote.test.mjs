// Unit tests for lib/quote.ts (Quote → the session's composer draft).
// Run: node --test src/lib/__tests__/quote.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { appendQuote, pickQuoteRun } from '../quote.ts'

test('appendQuote: into an empty draft, leaving a blank line to type below', () => {
  assert.equal(appendQuote('', 'hello'), '> hello\n\n')
})

test('appendQuote: every line is quoted, blank lines stay inside the quote', () => {
  assert.equal(appendQuote('', 'a\n\nb'), '> a\n>\n> b\n\n')
})

test('appendQuote: after typed text, separated by a blank line', () => {
  assert.equal(appendQuote('please check', 'x = 1'), 'please check\n\n> x = 1\n\n')
})

test('appendQuote: trailing whitespace of the draft collapses to one blank line', () => {
  assert.equal(appendQuote('note  \n\n\n', 'q'), 'note\n\n> q\n\n')
})

test('appendQuote: quoting twice gives two blocks one blank line apart', () => {
  const once = appendQuote('', 'first')
  assert.equal(appendQuote(once, 'second'), '> first\n\n> second\n\n')
})

test('appendQuote: text typed after a quote stays, the next quote goes below it', () => {
  assert.equal(appendQuote('> first\n\nwhy?', 'second'), '> first\n\nwhy?\n\n> second\n\n')
})

test('appendQuote: a blank quote leaves the draft untouched', () => {
  assert.equal(appendQuote('draft', ''), 'draft')
  assert.equal(appendQuote('draft', ' \n\t\n'), 'draft')
})

test('appendQuote: CRLF and surrounding blank lines are dropped, indentation kept', () => {
  assert.equal(appendQuote('', '\r\n\r\n  if (x) {\r\n    y()\r\n  }\r\n\r\n'), '>   if (x) {\n>     y()\n>   }\n\n')
})

test('appendQuote: a source is the first quoted line', () => {
  assert.equal(appendQuote('', 'const a = 1', { path: 'src/a.ts' }), '> `src/a.ts`\n> const a = 1\n\n')
})

test('appendQuote: a source line range, a single line shown once', () => {
  assert.equal(
    appendQuote('', 'x', { path: 'a.ts', lines: { start: 12, end: 20 } }),
    '> `a.ts:12-20`\n> x\n\n',
  )
  assert.equal(appendQuote('', 'x', { path: 'a.ts', lines: { start: 7, end: 7 } }), '> `a.ts:7`\n> x\n\n')
})

// ── pickQuoteRun: where a quote from a file window goes ─────────────────────
const at = (r) => r.at
const runs = [
  { id: 'old', at: '2026-11-01T00:00:00Z' },
  { id: 'new', at: '2026-11-07T00:00:00Z' },
  { id: 'worker', at: '2026-11-08T00:00:00Z', conductor_run_id: 'new' },
  { id: 'mid', at: '2026-11-05T00:00:00Z' },
]

test('pickQuoteRun: the run that opened the file wins, even an older one', () => {
  assert.equal(pickQuoteRun(runs, 'old', at)?.id, 'old')
})

test('pickQuoteRun: the opener may be a worker', () => {
  assert.equal(pickQuoteRun(runs, 'worker', at)?.id, 'worker')
})

test('pickQuoteRun: no opener → the latest session, workers skipped', () => {
  assert.equal(pickQuoteRun(runs, null, at)?.id, 'new')
  assert.equal(pickQuoteRun(runs, undefined, at)?.id, 'new')
})

test('pickQuoteRun: an opener that is gone → the latest session', () => {
  assert.equal(pickQuoteRun(runs, 'deleted', at)?.id, 'new')
})

test('pickQuoteRun: a project without sessions → none (Quote hidden)', () => {
  assert.equal(pickQuoteRun([], null, at), null)
  assert.equal(pickQuoteRun([], 'x', at), null)
  assert.equal(pickQuoteRun([{ id: 'w', at: 'x', conductor_run_id: 'c' }], null, at), null)
})
