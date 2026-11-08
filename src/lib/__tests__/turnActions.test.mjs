// Unit tests for lib/turnActions.ts (the row of buttons under each chat turn).
// Run: node --test src/lib/__tests__/turnActions.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { TURN_ACTION_FEATURES, turnActions, turnAnswerText, turnFooters } from '../turnActions.ts'

const user = (text) => ({ kind: 'user', text })
const text = (t) => ({ kind: 'text', text: t })
const tool = () => ({ kind: 'tool', id: 't', name: 'Bash', input: {} })
const result = (t, is_error = false) => ({ kind: 'result', text: t, is_error })

const rows = (entries, running = false) =>
  [...turnFooters(entries, running)].map(([at, f]) => [at, f.side, f.from, f.to, f.hasText])

test('turnFooters: question row under the prompt, answer row under the turn’s last entry', () => {
  const entries = [{ kind: 'system' }, user('q1'), text('a1'), result(''), user('q2'), text('a2')]
  assert.deepEqual(rows(entries), [
    [1, 'question', 1, 1, true],
    [3, 'answer', 2, 3, true],
    [4, 'question', 4, 4, true],
    [5, 'answer', 5, 5, true],
  ])
})

test('turnFooters: tool calls between answer texts stay in one turn, row after the last entry', () => {
  const entries = [user('q'), text('looking'), tool(), { kind: 'thinking', text: 'hm' }, tool(), text('done')]
  assert.deepEqual(rows(entries), [
    [0, 'question', 0, 0, true],
    [5, 'answer', 1, 5, true],
  ])
})

test('turnFooters: compact / injected / command entries do not start a new turn', () => {
  const entries = [
    user('q'),
    text('a'),
    { kind: 'compact', summary: 's' },
    { kind: 'injected', source: 'skill', text: 'body' },
    { kind: 'command', name: '/compact' },
    text('b'),
  ]
  assert.deepEqual(rows(entries), [
    [0, 'question', 0, 0, true],
    [5, 'answer', 1, 5, true],
  ])
})

test('turnFooters: a prompt with no answer yet has only its question row', () => {
  assert.deepEqual(rows([user('q1'), text('a1'), user('q2')]), [
    [0, 'question', 0, 0, true],
    [1, 'answer', 1, 1, true],
    [2, 'question', 2, 2, true],
  ])
})

test('turnFooters: an answer of tool calls only gets a row without text', () => {
  assert.deepEqual(rows([user('q'), tool(), result('')]), [
    [0, 'question', 0, 0, true],
    [2, 'answer', 1, 2, false],
  ])
})

test('turnFooters: an answer of errors / logs only gets no row', () => {
  const entries = [user('q'), { kind: 'error', text: 'boom' }, result('rate limited', true)]
  assert.deepEqual(rows(entries), [[0, 'question', 0, 0, true]])
})

test('turnFooters: while running, the last turn has no rows; earlier turns keep theirs', () => {
  const entries = [user('q1'), text('a1'), user('q2'), text('partial')]
  assert.deepEqual(rows(entries, true), [
    [0, 'question', 0, 0, true],
    [1, 'answer', 1, 1, true],
  ])
})

test('turnFooters: the tail of a turn whose prompt is outside the window still gets its answer row', () => {
  const entries = [text('rest of an earlier answer'), tool(), user('q'), text('a')]
  assert.deepEqual(rows(entries), [
    [1, 'answer', 0, 1, true],
    [2, 'question', 2, 2, true],
    [3, 'answer', 3, 3, true],
  ])
})

test('turnFooters: a system banner alone before the first prompt gets no row', () => {
  assert.deepEqual(rows([{ kind: 'system' }, user('q'), text('a')]), [
    [1, 'question', 1, 1, true],
    [2, 'answer', 2, 2, true],
  ])
})

test('turnFooters: an image-only prompt has a question row without text', () => {
  assert.deepEqual(rows([{ kind: 'user', text: '', images: [{}] }]), [[0, 'question', 0, 0, false]])
})

test('turnFooters: empty log has no rows', () => {
  assert.equal(turnFooters([], false).size, 0)
  assert.equal(turnFooters([], true).size, 0)
})

test('turnAnswerText: joins the answer texts as markdown, skipping tool calls and thinking', () => {
  const entries = [
    user('q'),
    text('Plan:\n\n| a | b |\n|---|---|\n| 1 | 2 |'),
    tool(),
    { kind: 'thinking', text: 'secret thoughts' },
    text('```ts\nconst x = 1\n```\n'),
  ]
  assert.equal(
    turnAnswerText(entries, 1, 4),
    'Plan:\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```ts\nconst x = 1\n```',
  )
})

test('turnAnswerText: keeps a result’s own text, drops a failed result and blank entries', () => {
  const entries = [text('a'), text('   '), result('final words'), result('boom', true), result('')]
  assert.equal(turnAnswerText(entries, 0, 4), 'a\n\nfinal words')
})

test('turnAnswerText: stays inside the range', () => {
  const entries = [text('before'), text('inside'), text('after')]
  assert.equal(turnAnswerText(entries, 1, 1), 'inside')
  assert.equal(turnAnswerText(entries, 2, 9), 'after')
})

const allOn = () => true
const allOff = () => false
const ctx = { hasText: true, hasQuoteTarget: true, canSaveNote: true }

test('turnActions: question offers Copy and Fullscreen, nothing behind ⋯', () => {
  assert.deepEqual(turnActions('question', allOn, ctx), { primary: ['copy', 'fullscreen'], overflow: [] })
})

test('turnActions: a question without text has no buttons', () => {
  assert.deepEqual(turnActions('question', allOn, { ...ctx, hasText: false }), { primary: [], overflow: [] })
})

test('turnActions: answer with everything on', () => {
  assert.deepEqual(turnActions('answer', allOn, ctx), {
    primary: ['copy', 'quote', 'fullscreen', 'fullscreenTurn'],
    overflow: ['saveNote'],
  })
})

test('turnActions: quote follows text_tools.selection.quote, notes follow productivity.notes', () => {
  const only = (id) => (f) => f === id
  assert.deepEqual(turnActions('answer', only(TURN_ACTION_FEATURES.saveNote), ctx), {
    primary: ['copy', 'fullscreen', 'fullscreenTurn'],
    overflow: ['saveNote'],
  })
  assert.deepEqual(turnActions('answer', only(TURN_ACTION_FEATURES.quote), ctx), {
    primary: ['copy', 'quote', 'fullscreen', 'fullscreenTurn'],
    overflow: [],
  })
  assert.equal(TURN_ACTION_FEATURES.quote, 'text_tools.selection.quote')
  assert.equal(TURN_ACTION_FEATURES.saveNote, 'productivity.notes')
})

test('turnActions: every gated feature off leaves the ungated buttons and no ⋯', () => {
  assert.deepEqual(turnActions('answer', allOff, ctx), {
    primary: ['copy', 'fullscreen', 'fullscreenTurn'],
    overflow: [],
  })
})

test('turnActions: no quote target hides Quote; a surface without capture hides the note', () => {
  assert.deepEqual(turnActions('answer', allOn, { ...ctx, hasQuoteTarget: false, canSaveNote: false }), {
    primary: ['copy', 'fullscreen', 'fullscreenTurn'],
    overflow: [],
  })
})

test('turnActions: an answer of tool calls only offers just Fullscreen turn', () => {
  assert.deepEqual(turnActions('answer', allOn, { ...ctx, hasText: false }), {
    primary: ['fullscreenTurn'],
    overflow: [],
  })
})
