// Unit tests for lib/editResend.ts ("Edit and resend" a sent message, UX-30).
// Run: node --test src/lib/__tests__/editResend.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  REWIND_ERRORS, beginEdit, canEditResend, editDraftOf, endEdit, rewindErrorKey, rewindStep,
} from '../editResend.ts'
import { composeWithQuotes, pendingRunQuotes, quoteSetViews } from '../quote.ts'

const run = (over = {}) => ({ status: 'done', engine: 'claude', ...over })
const idle = { running: false, sending: false }

test('canEditResend: a stopped Claude / Codex session that has run, where Rewind takes the conversation back', () => {
  for (const status of ['done', 'failed', 'cancelled']) {
    assert.equal(canEditResend(run({ status }), idle), true, status)
    assert.equal(canEditResend(run({ status, engine: 'codex' }), idle), true, `codex ${status}`)
  }
  assert.equal(canEditResend(run({ status: 'running' }), idle), false)
  assert.equal(canEditResend(run({ status: 'fetched' }), idle), false)
  assert.equal(canEditResend(run({ engine: 'shell' }), idle), false)
  assert.equal(canEditResend(null, idle), false)
})

test('canEditResend: hidden while running or sending, and for a Conductor worker', () => {
  assert.equal(canEditResend(run(), { running: true, sending: false }), false)
  assert.equal(canEditResend(run(), { running: false, sending: true }), false)
  assert.equal(canEditResend(run({ role: 'worker' }), idle), false)
  assert.equal(canEditResend(run({ conductor_run_id: 'c1' }), idle), false)
  assert.equal(canEditResend(run({ role: 'conductor', conductor_run_id: null }), idle), true)
})

const quoteRow = (over = {}) => ({
  id: 'q', run_id: 'r1', quote_set: 's', position: 0, kind: 'entry', source_run_id: 'r1', source_entry_id: 'msg_1',
  block_index: 0, start_offset: 4, end_offset: 7, path: null, line_start: null, line_end: null, excerpt: 'cat',
  note: '', created_at: 'c', source_offset: 120, ...over,
})

test('editDraftOf: the typed text, the quotes as cards again and the images; resending gives the same prompt', () => {
  let n = 0
  const rows = [
    quoteRow({ id: 'b', position: 1, kind: 'file', source_entry_id: null, path: 'src/a.ts', line_start: 3, line_end: 5, excerpt: 'let a = 1' }),
    quoteRow({ id: 'a', note: 'see' }),
  ]
  const text = composeWithQuotes([{ excerpt: 'cat', note: 'see' }, { excerpt: 'let a = 1', path: 'src/a.ts', lines: { start: 3, end: 5 } }], 'why?')
  const image = { media_type: 'image/png', data: 'AAA' }
  const draft = editDraftOf({ text, quotes: rows, images: [image] }, () => `id-${++n}`)
  assert.equal(draft.text, 'why?')
  assert.deepEqual(draft.quotes, [
    { id: 'id-1', kind: 'entry', excerpt: 'cat', note: 'see', run_id: 'r1', source_entry_id: 'msg_1', block_index: 0, start: 4, end: 7 },
    { id: 'id-2', kind: 'file', excerpt: 'let a = 1', path: 'src/a.ts', lines: { start: 3, end: 5 } },
  ])
  assert.deepEqual(draft.images, [image])
  assert.notEqual(draft.images[0], image)
  assert.equal(composeWithQuotes(draft.quotes, draft.text), text)
})

test('editDraftOf: a message without quotes or images is its text', () => {
  assert.deepEqual(editDraftOf({ text: 'fix the login', quotes: [] }, () => 'x'), { text: 'fix the login', quotes: [], images: [] })
})

test('editDraftOf: a quote whose source a Rewind cut still goes out with its excerpt; its card says it is gone', () => {
  const rows = [quoteRow({ source_offset: null })]
  const text = composeWithQuotes([{ excerpt: 'cat' }], 'again')
  const draft = editDraftOf({ text, quotes: rows }, () => 'q1')
  assert.equal(draft.quotes.length, 1)
  assert.equal(composeWithQuotes(draft.quotes, draft.text), text)
  // Resent: the source is no longer on screen, so the card is not pending.
  const sent = pendingRunQuotes('r1', 'set', draft.quotes).map((q) => ({ ...q, pending: false }))
  assert.equal(quoteSetViews(sent).get('set').cards[0].jumpable, false)
})

test('beginEdit / endEdit: the draft set aside comes back, nothing lost', () => {
  const draft = { text: 'half-written', images: [], files: [{ name: 'a', path: '/a' }] }
  const first = beginEdit(null, { runId: 'r1', turn: 3, number: 2 }, draft)
  assert.deepEqual(first, { runId: 'r1', turn: 3, number: 2, stash: draft, rewound: false })
  // Editing another message of the same session: the composer holds the first
  // edit by then, the draft set aside is still the original one.
  const second = beginEdit(first, { runId: 'r1', turn: 5, number: 4 }, { text: 'the first edit', images: [], files: [] })
  assert.equal(second.turn, 5)
  assert.equal(endEdit(second), draft)
  // Another session: its own draft.
  const other = { text: 'other', images: [], files: [] }
  assert.equal(endEdit(beginEdit(second, { runId: 'r2', turn: 1, number: null }, other)), other)
})

test('rewindStep: refuse when the conversation cannot go back, confirm when files would be restored', () => {
  const files = (changes = [], kept = []) => ({ changes, kept, fingerprint: 'f' })
  assert.equal(rewindStep({ conversation: true, files: files() }), 'rewind')
  assert.equal(rewindStep({ conversation: true, files: files([{ path: 'a', change: 'restore', outside: false }]) }), 'confirm')
  assert.equal(rewindStep({ conversation: true, files: files([], ['big.bin']) }), 'confirm')
  assert.equal(rewindStep({ conversation: false, files: files() }), 'refuse')
  assert.equal(rewindStep({ conversation: false, files: files([{ path: 'a' }]) }), 'refuse')
})

test('rewindErrorKey: known refusals get their message, anything else is shown as is', () => {
  for (const code of REWIND_ERRORS) assert.equal(rewindErrorKey(code), `run.rewind.errors.${code}`)
  assert.equal(rewindErrorKey('snapshot_damaged: blob abc'), 'run.rewind.errors.snapshot_damaged')
  assert.equal(rewindErrorKey(new Error('boom')), null)
})
