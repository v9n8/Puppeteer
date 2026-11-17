// Unit tests for lib/workerResults.ts (the worker result card in a conductor's
// conversation, V3-30). Run: node --test src/lib/__tests__/workerResults.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { register } from 'node:module'

register(
  'data:text/javascript,' +
    encodeURIComponent(`
      export async function resolve(spec, ctx, next) {
        try { return await next(spec, ctx) } catch (e) {
          if (spec.startsWith('.') && !/\\.[cm]?[jt]s$/.test(spec)) return next(spec + '.ts', ctx)
          throw e
        }
      }`),
)
const { attachCards, attachNudge, formatDuration, parseWorkerResults, shortWorkerId, workerResultLine } = await import('../workerResults.ts')
const { parseStreamLogWindow } = await import('../streamEvents.ts')

// A translator that shows the key and its params, so the test reads the structure.
const t = (key, p) => `${key.replace('conductor.result.', '')}${p ? JSON.stringify(p) : ''}`

test('shortWorkerId keeps 8 characters like the worker column', () => {
  assert.equal(shortWorkerId('91c0abcd-1111-2222'), '91c0abcd')
  assert.equal(shortWorkerId('ab'), 'ab')
})

test('formatDuration picks the largest two units and drops unusable values', () => {
  assert.equal(formatDuration(372000, t), 'durMinSec{"m":6,"s":12}')
  assert.equal(formatDuration(42000, t), 'durSec{"s":42}')
  assert.equal(formatDuration(3900000, t), 'durHourMin{"h":1,"m":5}')
  assert.equal(formatDuration(0, t), '')
  assert.equal(formatDuration(undefined, t), '')
  assert.equal(formatDuration(Number.NaN, t), '')
})

test('workerResultLine joins what it has and leaves out the rest', () => {
  const w = { id: 'a', title: 't', status: 'done', finished_at: '', duration_ms: 372000, total_tokens: 41000 }
  assert.equal(
    workerResultLine(w, { files: 3, added: 120, removed: 4 }, t),
    'edited{"n":3} · +120 −4 · durMinSec{"m":6,"s":12} · tokens{"n":"41k"}',
  )
  assert.equal(workerResultLine(w, { files: 0, added: 0, removed: 0 }, t), 'durMinSec{"m":6,"s":12} · tokens{"n":"41k"}')
  assert.equal(workerResultLine({ ...w, duration_ms: undefined, total_tokens: undefined }, null, t), '')
})

test('parseWorkerResults drops rows without an id and keeps absent numbers absent', () => {
  const got = parseWorkerResults({ workers: [{ id: 'w1', title: 'X', status: 'failed', finished_at: 'z', total_tokens: 5 }, { title: 'no id' }, null] })
  assert.deepEqual(got, [{ id: 'w1', title: 'X', status: 'failed', finished_at: 'z', total_tokens: 5 }])
  assert.deepEqual(parseWorkerResults({}), [])
})

test('attachNudge takes only a session nudge that directly follows a card record', () => {
  const nudge = { source: 'session', text: 'x' }
  const cards = [{ kind: 'worker_results' }]
  assert.equal(attachNudge(cards, nudge), true)
  assert.equal(cards[0].nudge, 'x')
  assert.equal(attachNudge([{ kind: 'text' }], nudge), false)
  assert.equal(attachNudge([{ kind: 'worker_results' }], { ...nudge, source: 'skill' }), false)
})

test('the log reader turns the record into a card entry and flags the nudge after it', () => {
  const rec = JSON.stringify({ type: '_puppeteer_worker_results', workers: [{ id: 'w1', title: 'A', status: 'done', finished_at: 'f' }] })
  const nudge = JSON.stringify({ type: 'user', message: { role: 'user', content: '[session] A worker you spawned has finished its turn.' } })
  const { entries } = parseStreamLogWindow([rec, nudge])
  assert.equal(entries[0].kind, 'worker_results')
  assert.equal(entries[0].workers[0].id, 'w1')
  assert.equal(entries.length, 1)
  assert.match(entries[0].nudge, /^A worker you spawned/)
  // An old log has no card record: the nudge stays an entry, with its button.
  assert.equal(parseStreamLogWindow([nudge]).entries[0].kind, 'injected')
})

test('a card record written after its nudge takes the nudge too', () => {
  const rec = JSON.stringify({ type: '_puppeteer_worker_results', workers: [{ id: 'w1', title: 'A', status: 'done', finished_at: 'f' }] })
  const nudge = JSON.stringify({ type: 'user', message: { role: 'user', content: '[session] A worker you spawned has finished its turn.' } })
  const { entries } = parseStreamLogWindow([nudge, rec])
  assert.equal(entries.length, 1)
  assert.equal(entries[0].kind, 'worker_results')
  assert.match(entries[0].nudge, /^A worker you spawned/)
  // Only a session nudge right before the record.
  assert.equal(attachCards([{ kind: 'text', text: 'x' }], []), false)
  assert.equal(attachCards([{ kind: 'injected', source: 'skill', text: 'x' }], []), false)
})
