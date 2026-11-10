// Unit tests for lib/messageQueue.ts (messages queued while a turn runs, UX-25).
// Run: node --test src/lib/__tests__/messageQueue.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  editQueued,
  emptyQueue,
  enqueue,
  onTurnEnd,
  queuedPreview,
  removeQueued,
  requeue,
  sanitizeQueue,
  takeQueued,
} from '../messageQueue.ts'

const draft = (text, extra = {}) => ({ text, images: [], files: [], ...extra })

function queueOf(...texts) {
  let q = emptyQueue()
  texts.forEach((t, i) => { q = enqueue(q, `m${i + 1}`, draft(t)) })
  return q
}

test('enqueue: appends in order and copies the draft', () => {
  const d = draft('first', { files: [{ name: 'a.ts', path: '/p/a.ts' }] })
  let q = enqueue(emptyQueue(), 'm1', d)
  q = enqueue(q, 'm2', draft('second'))
  d.files.push({ name: 'b.ts', path: '/p/b.ts' })
  assert.deepEqual(q.items.map((i) => i.id), ['m1', 'm2'])
  assert.equal(q.items[0].draft.files.length, 1)
})

test('enqueue: an empty draft is not queued, attachments alone are', () => {
  const q = emptyQueue()
  assert.equal(enqueue(q, 'm1', draft('   \n')), q)
  const img = { media_type: 'image/png', data: 'AAAA' }
  assert.equal(enqueue(q, 'm2', draft('', { images: [img] })).items.length, 1)
})

test('enqueue: quote text in the draft is kept as typed', () => {
  const q = enqueue(emptyQueue(), 'm1', draft('> quoted line\n\nwhy?'))
  assert.equal(q.items[0].draft.text, '> quoted line\n\nwhy?')
})

test('onTurnEnd: a successful turn sends the head only, the rest wait', () => {
  const r = onTurnEnd(queueOf('a', 'b', 'c'), { status: 'done', overBudget: false })
  assert.equal(r.send.id, 'm1')
  assert.deepEqual(r.queue.items.map((i) => i.id), ['m2', 'm3'])
  assert.equal(r.queue.hold, null)
  const next = onTurnEnd(r.queue, { status: 'done', overBudget: false })
  assert.equal(next.send.id, 'm2')
})

test('onTurnEnd: a failed turn keeps every message and holds', () => {
  const q = queueOf('a', 'b')
  const r = onTurnEnd(q, { status: 'failed', overBudget: false })
  assert.equal(r.send, null)
  assert.deepEqual(r.queue.items, q.items)
  assert.equal(r.queue.hold, 'failed')
})

test('onTurnEnd: a cancelled turn keeps every message and holds', () => {
  const r = onTurnEnd(queueOf('a'), { status: 'cancelled', overBudget: false })
  assert.equal(r.send, null)
  assert.equal(r.queue.items.length, 1)
  assert.equal(r.queue.hold, 'cancelled')
})

test('onTurnEnd: a blocked budget keeps the queue', () => {
  const r = onTurnEnd(queueOf('a', 'b'), { status: 'done', overBudget: true })
  assert.equal(r.send, null)
  assert.equal(r.queue.items.length, 2)
  assert.equal(r.queue.hold, 'budget')
})

test('onTurnEnd: the next successful turn end clears a hold and sends', () => {
  const held = onTurnEnd(queueOf('a', 'b'), { status: 'failed', overBudget: false }).queue
  const r = onTurnEnd(held, { status: 'done', overBudget: false })
  assert.equal(r.send.id, 'm1')
  assert.equal(r.queue.hold, null)
})

test('onTurnEnd: an empty queue sends nothing and drops a stale hold', () => {
  const r = onTurnEnd({ items: [], hold: 'failed' }, { status: 'done', overBudget: false })
  assert.equal(r.send, null)
  assert.deepEqual(r.queue, emptyQueue())
})

test('onTurnEnd: a status that is not a turn end changes nothing', () => {
  const q = queueOf('a')
  const r = onTurnEnd(q, { status: 'running', overBudget: false })
  assert.equal(r.send, null)
  assert.equal(r.queue, q)
})

test('editQueued: changes only that message and keeps its attachments', () => {
  const img = { media_type: 'image/png', data: 'AAAA' }
  let q = enqueue(queueOf('a'), 'm2', draft('b', { images: [img] }))
  q = editQueued(q, 'm2', 'b edited')
  assert.equal(q.items[0].draft.text, 'a')
  assert.equal(q.items[1].draft.text, 'b edited')
  assert.deepEqual(q.items[1].draft.images, [img])
})

test('editQueued: emptying a text-only message removes it', () => {
  const q = editQueued(queueOf('a', 'b'), 'm1', '  ')
  assert.deepEqual(q.items.map((i) => i.id), ['m2'])
})

test('removeQueued: drops one message, keeps the order of the rest', () => {
  const q = removeQueued(queueOf('a', 'b', 'c'), 'm2')
  assert.deepEqual(q.items.map((i) => i.id), ['m1', 'm3'])
})

test('takeQueued: removes the message and clears the hold', () => {
  const held = { ...queueOf('a', 'b'), hold: 'cancelled' }
  const r = takeQueued(held, 'm2')
  assert.equal(r.item.id, 'm2')
  assert.deepEqual(r.queue.items.map((i) => i.id), ['m1'])
  assert.equal(r.queue.hold, null)
  assert.equal(takeQueued(held, 'nope').item, null)
})

test('requeue: an unsent message goes back to the head, held', () => {
  const { queue, send } = onTurnEnd(queueOf('a', 'b'), { status: 'done', overBudget: false })
  const q = requeue(queue, send, 'budget')
  assert.deepEqual(q.items.map((i) => i.id), ['m1', 'm2'])
  assert.equal(q.hold, 'budget')
  assert.equal(requeue(queue, send, null).hold, null)
})

test('sanitizeQueue: round-trips the persisted shape', () => {
  const img = { media_type: 'image/png', data: 'AAAA' }
  const q = { ...enqueue(queueOf('a'), 'm2', draft('', { images: [img] })), hold: 'failed' }
  assert.deepEqual(sanitizeQueue(JSON.parse(JSON.stringify(q))), q)
})

test('sanitizeQueue: drops broken items, duplicates and unknown holds', () => {
  const raw = {
    items: [
      { id: 'm1', draft: { text: 'ok', images: [{ media_type: 1 }], files: 'x' } },
      { id: 'm1', draft: { text: 'duplicate' } },
      { id: '', draft: { text: 'no id' } },
      { id: 'm2', draft: { text: '' } },
      null,
    ],
    hold: 'whatever',
  }
  assert.deepEqual(sanitizeQueue(raw), {
    items: [{ id: 'm1', draft: { text: 'ok', images: [], files: [] } }],
    hold: null,
  })
  assert.equal(sanitizeQueue({ items: [] }), null)
  assert.equal(sanitizeQueue('junk'), null)
})

test('queuedPreview: first typed line, else the quoted text', () => {
  const item = (text) => ({ id: 'm', draft: draft(text) })
  assert.equal(queuedPreview(item('> source\n\nwhy is this?\nmore')), 'why is this?')
  assert.equal(queuedPreview(item('> only a quote\n>\n> second')), 'only a quote')
  assert.equal(queuedPreview(item('')), '')
})
