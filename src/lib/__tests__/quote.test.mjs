// Unit tests for lib/quote.ts (Quote → the session's composer draft).
// Run: node --test src/lib/__tests__/quote.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  appendQuote, composeWithQuotes, draftFromSentPrompt, entryOrTextQuote, fileQuote, pendingRunQuotes, pickQuoteRun,
  quoteInput, quoteSetViews, sanitizeDraftQuotes, serializeQuotes, stripQuotePrefix,
} from '../quote.ts'

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

// ── Structured quotes (UX-29) ───────────────────────────────────────────────
const anchor = { entryId: 'msg_1', block: 0, start: 4, end: 7 }
const entryQ = entryOrTextQuote('q1', 'cat', 'r1', anchor)
const fileQ = fileQuote('q2', 'let a = 1', { path: 'src/a.ts', lines: { start: 3, end: 5 } })
const textQ = entryOrTextQuote('q3', 'my own question', 'r1', null)

test('entryOrTextQuote: anchored to a message, or plain text without an anchor', () => {
  assert.deepEqual(entryQ, {
    id: 'q1', kind: 'entry', excerpt: 'cat', run_id: 'r1', source_entry_id: 'msg_1', block_index: 0, start: 4, end: 7,
  })
  assert.deepEqual(textQ, { id: 'q3', kind: 'text', excerpt: 'my own question' })
})

test('wire format unchanged: the prompt is what quoting into the draft used to give', () => {
  // Before UX-29 each Quote ran appendQuote on the draft, then the user typed below.
  let old = appendQuote('', 'cat')
  old = appendQuote(old, 'let a = 1', { path: 'src/a.ts', lines: { start: 3, end: 5 } })
  old = appendQuote(old, 'my own question')
  const before = `${old}why is this?`
  assert.equal(composeWithQuotes([entryQ, fileQ, textQ], 'why is this?'), before)
  assert.equal(before, '> cat\n\n> `src/a.ts:3-5`\n> let a = 1\n\n> my own question\n\nwhy is this?')
  // Only quotes: the trailing blank lines go, as the send trims.
  assert.equal(composeWithQuotes([entryQ], ''), '> cat')
  // No quotes: the text as typed.
  assert.equal(composeWithQuotes([], 'hello'), 'hello')
})

test('a note goes right below its quote', () => {
  const noted = { ...entryQ, note: '  this one  ' }
  assert.equal(composeWithQuotes([noted, fileQ], 'ok'), '> cat\n\nthis one\n\n> `src/a.ts:3-5`\n> let a = 1\n\nok')
})

test('stripQuotePrefix: the bubble keeps what was typed after the quotes', () => {
  const prefix = serializeQuotes([entryQ, fileQ])
  assert.equal(stripQuotePrefix(composeWithQuotes([entryQ, fileQ], 'why?'), prefix), 'why?')
  assert.equal(stripQuotePrefix(composeWithQuotes([entryQ, fileQ], ''), prefix), '')
  assert.equal(stripQuotePrefix('something else', prefix), null)
  assert.equal(stripQuotePrefix('plain', ''), 'plain')
})

test('quoteInput: what the send saves for each kind', () => {
  assert.deepEqual(quoteInput({ ...entryQ, note: ' n ' }), {
    kind: 'entry', source_run_id: 'r1', source_entry_id: 'msg_1', block_index: 0, start_offset: 4, end_offset: 7,
    excerpt: 'cat', note: 'n',
  })
  assert.deepEqual(quoteInput(fileQ), { kind: 'file', path: 'src/a.ts', line_start: 3, line_end: 5, excerpt: 'let a = 1', note: '' })
  assert.deepEqual(quoteInput(textQ), { kind: 'text', excerpt: 'my own question', note: '' })
})

const row = (over) => ({
  id: 'x', run_id: 'r1', quote_set: 's1', position: 0, kind: 'entry', source_run_id: 'r1', source_entry_id: 'msg_1',
  block_index: 0, start_offset: 4, end_offset: 7, path: null, line_start: null, line_end: null, excerpt: 'cat', note: '',
  created_at: 'c', source_offset: 120, ...over,
})

test('quoteSetViews: cards per prompt in order; a gone source is not clickable', () => {
  const views = quoteSetViews([
    row({ id: 'b', position: 1, kind: 'file', source_entry_id: null, path: 'src/a.ts', line_start: 3, line_end: 5, excerpt: 'let a = 1', source_offset: null }),
    row({ id: 'a', position: 0 }),
    row({ id: 'c', quote_set: 's2', source_offset: null }),
    row({ id: 'd', quote_set: 's2', position: 1, source_offset: null, pending: true }),
  ])
  const s1 = views.get('s1')
  assert.deepEqual(s1.cards.map((c) => [c.quote.id, c.jumpable]), [['a', true], ['b', true]])
  assert.equal(s1.prefix, serializeQuotes([entryQ, fileQ]))
  assert.deepEqual(views.get('s2').cards.map((c) => [c.quote.id, c.jumpable]), [['c', false], ['d', true]])
})

test('pendingRunQuotes: cards for a prompt just sent, entry sources counted as there', () => {
  const rows = pendingRunQuotes('r1', 's9', [entryQ, fileQ])
  assert.deepEqual(rows.map((r) => [r.quote_set, r.position, r.kind, r.pending]), [['s9', 0, 'entry', true], ['s9', 1, 'file', false]])
  const view = quoteSetViews(rows).get('s9')
  assert.deepEqual(view.cards.map((c) => c.jumpable), [true, true])
})

test('draftFromSentPrompt (for UX-30): typed text and quote cards back, ids fresh', () => {
  let n = 0
  const newId = () => `new-${++n}`
  const sent = composeWithQuotes([{ ...entryQ, note: 'see' }, fileQ], 'why?')
  const rows = [
    row({ id: 'f', position: 1, kind: 'file', source_entry_id: null, path: 'src/a.ts', line_start: 3, line_end: 5, excerpt: 'let a = 1' }),
    row({ id: 'e', position: 0, note: 'see' }),
  ]
  const draft = draftFromSentPrompt(sent, rows, newId)
  assert.equal(draft.text, 'why?')
  assert.deepEqual(draft.quotes, [
    { id: 'new-1', kind: 'entry', excerpt: 'cat', note: 'see', run_id: 'r1', source_entry_id: 'msg_1', block_index: 0, start: 4, end: 7 },
    { id: 'new-2', kind: 'file', excerpt: 'let a = 1', path: 'src/a.ts', lines: { start: 3, end: 5 } },
  ])
  // Sending the rebuilt draft gives the same prompt again.
  assert.equal(composeWithQuotes(draft.quotes, draft.text), sent)
  // A text that no longer starts with its quotes keeps them inside, nothing doubled.
  assert.deepEqual(draftFromSentPrompt('edited elsewhere', rows, newId), { text: 'edited elsewhere', quotes: [] })
})

test('sanitizeDraftQuotes: stored quotes survive a reload, malformed ones are dropped', () => {
  const stored = JSON.parse(JSON.stringify([{ ...entryQ, note: 'n' }, fileQ, textQ]))
  assert.deepEqual(sanitizeDraftQuotes(stored), [{ ...entryQ, note: 'n' }, fileQ, textQ])
  assert.deepEqual(sanitizeDraftQuotes([
    { id: 'a', kind: 'entry', excerpt: 'x', run_id: 'r', source_entry_id: 'm', block_index: 0, start: 5, end: 2 },
    { id: 'b', kind: 'file', excerpt: 'x' },
    { id: '', kind: 'text', excerpt: 'x' },
    { id: 'c', kind: 'text', excerpt: '  ' },
    { id: 'd', kind: 'other', excerpt: 'x' },
    null,
  ]), [])
  assert.deepEqual(sanitizeDraftQuotes('nope'), [])
})
